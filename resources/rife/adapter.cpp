// Fold's private, versioned C ABI. Only GPU buffers cross the runtime boundary.
#include "command.h"
#include "gpu.h"
#include "net.h"
#include "rife_ops.h"
#include <algorithm>
#include <array>
#include <cstring>
#include <memory>
#include <mutex>
#include <stdexcept>
#include <string>
#include <unistd.h>
#include <unordered_map>
#include <vector>
#include <vulkan/vulkan.h>

#define API extern "C" __attribute__((visibility("default")))
DEFINE_LAYER_CREATOR(Warp)
static std::mutex startup;
static thread_local std::string last_error;
static void check(VkResult v) {
  if (v != VK_SUCCESS)
    throw std::runtime_error("Vulkan RIFE error " + std::to_string(v));
}
struct Descriptor {
  uint64_t bytes, allocation;
  uint32_t memory_type;
  int32_t fd;
};
class Exchange : public ncnn::VkAllocator {
public:
  std::unordered_map<ncnn::VkBufferMemory *, Descriptor> records;
  explicit Exchange(const ncnn::VulkanDevice *d) : VkAllocator(d) {
    mappable = false;
    coherent = false;
  }
  ncnn::VkBufferMemory *fastMalloc(size_t size) override {
    auto p = std::make_unique<ncnn::VkBufferMemory>();
    std::memset(p.get(), 0, sizeof(*p));
    VkExternalMemoryBufferCreateInfo external{
        VK_STRUCTURE_TYPE_EXTERNAL_MEMORY_BUFFER_CREATE_INFO};
    external.handleTypes = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD_BIT;
    VkBufferCreateInfo info{VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO};
    info.pNext = &external;
    info.size = size;
    info.usage = VK_BUFFER_USAGE_STORAGE_BUFFER_BIT |
                 VK_BUFFER_USAGE_TRANSFER_SRC_BIT |
                 VK_BUFFER_USAGE_TRANSFER_DST_BIT;
    check(vkCreateBuffer(vkdev->vkdevice(), &info, nullptr, &p->buffer));
    VkMemoryRequirements req;
    vkGetBufferMemoryRequirements(vkdev->vkdevice(), p->buffer, &req);
    VkPhysicalDeviceMemoryProperties props;
    vkGetPhysicalDeviceMemoryProperties(vkdev->info.physical_device(), &props);
    uint32_t type = 0;
    while (type < props.memoryTypeCount &&
           (!(req.memoryTypeBits & (1u << type)) ||
            !(props.memoryTypes[type].propertyFlags &
              VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT)))
      ++type;
    if (type == props.memoryTypeCount) {
      vkDestroyBuffer(vkdev->vkdevice(), p->buffer, nullptr);
      throw std::runtime_error("No device-local RIFE memory");
    }
    VkExportMemoryAllocateInfo export_info{
        VK_STRUCTURE_TYPE_EXPORT_MEMORY_ALLOCATE_INFO};
    export_info.handleTypes = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD_BIT;
    VkMemoryDedicatedAllocateInfo dedicated{
        VK_STRUCTURE_TYPE_MEMORY_DEDICATED_ALLOCATE_INFO};
    dedicated.buffer = p->buffer;
    dedicated.pNext = &export_info;
    VkMemoryAllocateInfo alloc{VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
    alloc.pNext = &dedicated;
    alloc.allocationSize = req.size;
    alloc.memoryTypeIndex = type;
    auto result =
        vkAllocateMemory(vkdev->vkdevice(), &alloc, nullptr, &p->memory);
    if (result != VK_SUCCESS) {
      vkDestroyBuffer(vkdev->vkdevice(), p->buffer, nullptr);
      check(result);
    }
    result = vkBindBufferMemory(vkdev->vkdevice(), p->buffer, p->memory, 0);
    if (result != VK_SUCCESS) {
      vkDestroyBuffer(vkdev->vkdevice(), p->buffer, nullptr);
      vkFreeMemory(vkdev->vkdevice(), p->memory, nullptr);
      check(result);
    }
    p->capacity = size;
    p->stage_flags = VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT;
    records.emplace(p.get(), Descriptor{size, req.size, type, -1});
    return p.release();
  }
  void fastFree(ncnn::VkBufferMemory *p) override {
    if (!p)
      return;
    vkDestroyBuffer(vkdev->vkdevice(), p->buffer, nullptr);
    vkFreeMemory(vkdev->vkdevice(), p->memory, nullptr);
    records.erase(p);
    delete p;
  }
  ncnn::VkImageMemory *fastMalloc(int, int, int, size_t, int) override {
    return nullptr;
  }
  void fastFree(ncnn::VkImageMemory *) override {}
  Descriptor descriptor(ncnn::VkMat &m) {
    auto d = records.at(m.data);
    VkMemoryGetFdInfoKHR i{VK_STRUCTURE_TYPE_MEMORY_GET_FD_INFO_KHR};
    i.memory = m.data->memory;
    i.handleType = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD_BIT;
    auto get = reinterpret_cast<PFN_vkGetMemoryFdKHR>(
        vkGetDeviceProcAddr(vkdev->vkdevice(), "vkGetMemoryFdKHR"));
    if (!get)
      throw std::runtime_error("RIFE device lacks external memory FD support");
    check(get(vkdev->vkdevice(), &i, &d.fd));
    return d;
  }
};
// Explicit aggregate cap on transient tensors, including idle pooled
// allocations.
class Scratch : public ncnn::VkAllocator {
  std::vector<ncnn::VkBufferMemory *> idle;
  std::unordered_map<ncnn::VkBufferMemory *, size_t> owned;
  size_t allocated = 0;
  void destroy(ncnn::VkBufferMemory *p) {
    allocated -= owned.at(p);
    owned.erase(p);
    vkDestroyBuffer(vkdev->vkdevice(), p->buffer, nullptr);
    vkFreeMemory(vkdev->vkdevice(), p->memory, nullptr);
    delete p;
  }

public:
  size_t limit = 0;
  explicit Scratch(const ncnn::VulkanDevice *d) : VkAllocator(d) {
    mappable = false;
    coherent = false;
  }
  ~Scratch() { clear(); }
  void clear() override {
    for (auto p : idle)
      destroy(p);
    idle.clear();
  }
  ncnn::VkBufferMemory *fastMalloc(size_t size) override {
    auto found =
        std::min_element(idle.begin(), idle.end(), [&](auto a, auto b) {
          return (a->capacity >= size ? a->capacity : SIZE_MAX) <
                 (b->capacity >= size ? b->capacity : SIZE_MAX);
        });
    if (found != idle.end() && (*found)->capacity >= size) {
      auto p = *found;
      idle.erase(found);
      p->refcount = 0;
      return p;
    }
    auto p = std::make_unique<ncnn::VkBufferMemory>();
    std::memset(p.get(), 0, sizeof(*p));
    p->buffer = create_buffer(size, VK_BUFFER_USAGE_STORAGE_BUFFER_BIT |
                                        VK_BUFFER_USAGE_TRANSFER_SRC_BIT |
                                        VK_BUFFER_USAGE_TRANSFER_DST_BIT);
    if (!p->buffer)
      return nullptr;
    VkMemoryRequirements req;
    vkGetBufferMemoryRequirements(vkdev->vkdevice(), p->buffer, &req);
    while (allocated + req.size > limit && !idle.empty()) {
      auto old = idle.back();
      idle.pop_back();
      destroy(old);
    }
    if (allocated + req.size > limit) {
      vkDestroyBuffer(vkdev->vkdevice(), p->buffer, nullptr);
      return nullptr;
    }
    uint32_t type = vkdev->find_memory_index(
        req.memoryTypeBits, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, 0, 0);
    p->memory = allocate_memory(req.size, type);
    if (!p->memory) {
      vkDestroyBuffer(vkdev->vkdevice(), p->buffer, nullptr);
      return nullptr;
    }
    if (vkBindBufferMemory(vkdev->vkdevice(), p->buffer, p->memory, 0) !=
        VK_SUCCESS) {
      vkDestroyBuffer(vkdev->vkdevice(), p->buffer, nullptr);
      vkFreeMemory(vkdev->vkdevice(), p->memory, nullptr);
      return nullptr;
    }
    p->capacity = size;
    p->stage_flags = VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT;
    allocated += req.size;
    owned.emplace(p.get(), req.size);
    return p.release();
  }
  void fastFree(ncnn::VkBufferMemory *p) override {
    if (p)
      idle.push_back(p);
  }
  ncnn::VkImageMemory *fastMalloc(int, int, int, size_t, int) override {
    return nullptr;
  }
  void fastFree(ncnn::VkImageMemory *) override {}
};
struct Runtime {
  const ncnn::VulkanDevice *device = nullptr;
  ncnn::Net net;
  std::unique_ptr<Exchange> exchange;
  std::array<ncnn::VkMat, 5> buffers;
  std::unique_ptr<Scratch> scratch;
  std::unique_ptr<ncnn::VkStagingAllocator> staging;
  int width = 0, height = 0;
  ~Runtime() {
    if (device)
      vkDeviceWaitIdle(device->vkdevice());
    for (auto &b : buffers)
      b.release();
    net.clear();
  }
  void ownership(bool acquire, bool initial = false) {
    auto raw = device->vkdevice();
    uint32_t family = device->info.compute_queue_family_index();
    VkCommandPoolCreateInfo pi{VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO};
    pi.queueFamilyIndex = family;
    VkCommandPool pool;
    check(vkCreateCommandPool(raw, &pi, nullptr, &pool));
    VkFence fence = VK_NULL_HANDLE;
    VkQueue queue = VK_NULL_HANDLE;
    try {
      VkCommandBufferAllocateInfo ai{
          VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO};
      ai.commandPool = pool;
      ai.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
      ai.commandBufferCount = 1;
      VkCommandBuffer cmd;
      check(vkAllocateCommandBuffers(raw, &ai, &cmd));
      VkCommandBufferBeginInfo bi{VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};
      bi.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
      check(vkBeginCommandBuffer(cmd, &bi));
      std::array<VkBufferMemoryBarrier, 5> barriers{};
      for (size_t i = 0; i < 5; i++) {
        auto &b = barriers[i];
        b.sType = VK_STRUCTURE_TYPE_BUFFER_MEMORY_BARRIER;
        b.buffer = buffers[i].buffer();
        b.size = VK_WHOLE_SIZE;
        b.srcQueueFamilyIndex = acquire ? VK_QUEUE_FAMILY_EXTERNAL : family;
        b.dstQueueFamilyIndex = acquire ? family : VK_QUEUE_FAMILY_EXTERNAL;
        b.srcAccessMask = acquire || initial ? 0
                                             : VK_ACCESS_MEMORY_READ_BIT |
                                                   VK_ACCESS_MEMORY_WRITE_BIT;
        b.dstAccessMask = acquire ? VK_ACCESS_SHADER_READ_BIT |
                                        VK_ACCESS_SHADER_WRITE_BIT |
                                        VK_ACCESS_TRANSFER_WRITE_BIT
                                  : 0;
      }
      vkCmdPipelineBarrier(cmd,
                           acquire || initial
                               ? VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT
                               : VK_PIPELINE_STAGE_ALL_COMMANDS_BIT,
                           acquire ? VK_PIPELINE_STAGE_ALL_COMMANDS_BIT
                                   : VK_PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT,
                           0, 0, nullptr, 5, barriers.data(), 0, nullptr);
      check(vkEndCommandBuffer(cmd));
      VkFenceCreateInfo fi{VK_STRUCTURE_TYPE_FENCE_CREATE_INFO};
      check(vkCreateFence(raw, &fi, nullptr, &fence));
      queue = device->acquire_queue(family);
      if (!queue)
        throw std::runtime_error("RIFE queue unavailable");
      VkSubmitInfo si{VK_STRUCTURE_TYPE_SUBMIT_INFO};
      si.commandBufferCount = 1;
      si.pCommandBuffers = &cmd;
      check(vkQueueSubmit(queue, 1, &si, fence));
      check(vkWaitForFences(raw, 1, &fence, VK_TRUE, UINT64_MAX));
      device->reclaim_queue(family, queue);
      queue = VK_NULL_HANDLE;
    } catch (...) {
      vkDeviceWaitIdle(raw);
      if (queue)
        device->reclaim_queue(family, queue);
      if (fence)
        vkDestroyFence(raw, fence, nullptr);
      vkDestroyCommandPool(raw, pool, nullptr);
      throw;
    }
    vkDestroyFence(raw, fence, nullptr);
    vkDestroyCommandPool(raw, pool, nullptr);
    if (acquire)
      for (auto &b : buffers) {
        b.data->access_flags = VK_ACCESS_MEMORY_WRITE_BIT;
        b.data->stage_flags = VK_PIPELINE_STAGE_ALL_COMMANDS_BIT;
      }
  }
};
API const char *fold_rife_error() { return last_error.c_str(); }
API uint32_t fold_rife_abi() { return 1; }
API void *fold_rife_create(const uint8_t *uuid, const char *model) {
  try {
    std::lock_guard<std::mutex> lock(startup);
    ncnn::create_gpu_instance();
    int selected = -1;
    for (int i = 0; i < ncnn::get_gpu_count(); i++) {
      VkPhysicalDeviceIDProperties ids{
          VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_ID_PROPERTIES};
      VkPhysicalDeviceProperties2 props{
          VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2};
      props.pNext = &ids;
      vkGetPhysicalDeviceProperties2(ncnn::get_gpu_info(i).physical_device(),
                                     &props);
      if (std::memcmp(ids.deviceUUID, uuid, 16) == 0) {
        selected = i;
        break;
      }
    }
    if (selected < 0)
      throw std::runtime_error("RIFE cannot find Fold's Vulkan GPU");
    auto r = std::make_unique<Runtime>();
    r->device = ncnn::get_gpu_device(selected);
    r->exchange = std::make_unique<Exchange>(r->device);
    r->scratch = std::make_unique<Scratch>(r->device);
    r->staging = std::make_unique<ncnn::VkStagingAllocator>(r->device);
    auto &opt = r->net.opt;
    opt.use_vulkan_compute = true;
    opt.use_fp16_packed = false;
    opt.use_fp16_storage = false;
    opt.use_fp16_arithmetic = false;
    opt.use_int8_storage = false;
    opt.use_packing_layout = false;
    opt.use_image_storage = false;
    opt.num_threads = 1;
    r->net.set_vulkan_device(r->device);
    r->net.register_custom_layer("rife.Warp", Warp_layer_creator);
    if (r->net.load_param((std::string(model) + "/flownet.param").c_str()) ||
        r->net.load_model((std::string(model) + "/flownet.bin").c_str()))
      throw std::runtime_error("Cannot load pinned RIFE v4.6 model");
    return r.release();
  } catch (const std::exception &e) {
    last_error = e.what();
    return nullptr;
  }
}
API void fold_rife_destroy(void *ptr) { delete static_cast<Runtime *>(ptr); }
API int fold_rife_prepare(void *ptr, uint32_t w, uint32_t h,
                          Descriptor *descriptors) {
  try {
    auto &r = *static_cast<Runtime *>(ptr);
    if (w == 0 || h == 0 || w % 32 || h % 32 || uint64_t(w) * h > 16777216)
      throw std::runtime_error("Invalid padded RIFE dimensions");
    for (auto &b : r.buffers)
      b.release();
    r.scratch->clear();
    r.width = w;
    r.height = h;
    r.scratch->limit = 128ull * 1024 * 1024 + uint64_t(w) * h * 400;
    int channels[5] = {3, 3, 1, 4, 1};
    for (int i = 0; i < 5; i++) {
      r.buffers[i].create(w, h, channels[i], size_t(4), 1, r.exchange.get());
      if (r.buffers[i].empty())
        throw std::runtime_error("RIFE buffer allocation failed");
    }
    r.ownership(false, true);
    for (int i = 0; i < 5; i++)
      descriptors[i] = r.exchange->descriptor(r.buffers[i]);
    return 0;
  } catch (const std::exception &e) {
    last_error = e.what();
    return -1;
  }
}
API int fold_rife_run(void *ptr) {
  try {
    auto &r = *static_cast<Runtime *>(ptr);
    r.ownership(true);
    {
      ncnn::VkCompute cmd(r.device);
      auto ex = r.net.create_extractor();
      ex.set_blob_vkallocator(r.scratch.get());
      ex.set_workspace_vkallocator(r.scratch.get());
      ex.set_staging_vkallocator(r.staging.get());
      ex.input("in0", r.buffers[0]);
      ex.input("in1", r.buffers[1]);
      ex.input("in2", r.buffers[2]);
      ncnn::VkMat flow, mask;
      if (ex.extract("327", flow, cmd) || ex.extract("332", mask, cmd))
        throw std::runtime_error("RIFE flow/mask inference failed");
      if (flow.elempack != 1) {
        ncnn::VkMat unpacked;
        auto opt = r.net.opt;
        opt.blob_vkallocator = r.scratch.get();
        opt.workspace_vkallocator = r.scratch.get();
        r.device->convert_packing(flow, unpacked, 1, cmd, opt);
        flow = unpacked;
      }
      if (flow.w != r.width || flow.h != r.height || flow.c != 4 ||
          flow.elempack != 1 || flow.elemsize != 4 || mask.c != 1 ||
          mask.elempack != 1 || mask.elemsize != 4)
        throw std::runtime_error(
            "Unexpected RIFE model tensor layout: flow=" +
            std::to_string(flow.w) + "x" + std::to_string(flow.h) + "x" +
            std::to_string(flow.c) + " pack=" + std::to_string(flow.elempack) +
            " bytes=" + std::to_string(flow.elemsize) + " mask=" +
            std::to_string(mask.w) + "x" + std::to_string(mask.h) + "x" +
            std::to_string(mask.c) + " pack=" + std::to_string(mask.elempack) +
            " bytes=" + std::to_string(mask.elemsize));
      auto opt = r.net.opt;
      opt.blob_vkallocator = r.exchange.get();
      opt.workspace_vkallocator = r.exchange.get();
      cmd.record_clone(flow, r.buffers[3], opt);
      cmd.record_clone(mask, r.buffers[4], opt);
      if (cmd.submit_and_wait() != 0)
        throw std::runtime_error("RIFE GPU execution failed");
    }
    r.ownership(false);
    return 0;
  } catch (const std::exception &e) {
    last_error = e.what();
    return -1;
  }
}
