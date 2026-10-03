// Private ABI v1. Never propagate a C++ exception or expose OCIO's C++ ABI to Rust.
#include <OpenColorIO/OpenColorIO.h>
#include <cstdio>
#include <memory>
#include <stdexcept>
#include <string>
#include <sstream>
#include <iomanip>
#include <cmath>
namespace OCIO = OCIO_NAMESPACE;
namespace {
struct Config { OCIO::ConstConfigRcPtr value; OCIO::ConstContextRcPtr context; };
struct Processor { OCIO::ConstProcessorRcPtr value; OCIO::ConstCPUProcessorRcPtr cpu; };
void error(char *out, size_t size, const char *message) noexcept {
    if (size) std::snprintf(out, size, "%s", message);
}
template<class F> void *create(F fn, char *out, size_t size) noexcept {
    try { return fn(); }
    catch (const std::exception &e) { error(out, size, e.what()); }
    catch (...) { error(out, size, "unknown OCIO exception"); }
    return nullptr;
}
}
extern "C" {
unsigned fold_ocio_abi() noexcept { return 1; }
const char *fold_ocio_version() noexcept { return OCIO::GetVersion(); }
void *fold_ocio_config(const char *path, char *err, size_t size) noexcept {
    return create([&]() -> void * {
        auto config = OCIO::Config::CreateFromFile(path);
        // Implicit environment-dependent resources cannot be reproducibly pinned.
        if (config->getNumEnvironmentVars() != 0)
            throw std::runtime_error("config environment variables are unsupported; use explicit resources");
        config->validate();
        auto context = config->getCurrentContext()->createEditableCopy();
        context->clearStringVars();
        return new Config{config, context};
    }, err, size);
}
void fold_ocio_config_drop(void *p) noexcept { delete static_cast<Config *>(p); }
// kind: 0 color spaces, 1 displays, 2 views of display, 3 looks.
int fold_ocio_count(void *p, int kind, const char *display) noexcept {
    try {
        auto c = static_cast<Config *>(p)->value;
        switch (kind) {
            case 0: return c->getNumColorSpaces(OCIO::SEARCH_REFERENCE_SPACE_SCENE, OCIO::COLORSPACE_ACTIVE);
            case 1: return c->getNumDisplays();
            case 2: return c->getNumViews(display);
            case 3: return c->getNumLooks();
        }
    } catch (...) {}
    return -1;
}
const char *fold_ocio_name(void *p, int kind, const char *display, int index) noexcept {
    try {
        auto c = static_cast<Config *>(p)->value;
        switch (kind) {
            case 0: return c->getColorSpaceNameByIndex(OCIO::SEARCH_REFERENCE_SPACE_SCENE, OCIO::COLORSPACE_ACTIVE, index);
            case 1: return c->getDisplay(index);
            case 2: return c->getView(display, index);
            case 3: return c->getLookNameByIndex(index);
        }
    } catch (...) {}
    return nullptr;
}
void *fold_ocio_processor(void *p, const char *source, const char *destination,
                          const char *view, const char *look, char *err, size_t size) noexcept {
    return create([&]() -> void * {
        auto c = static_cast<Config *>(p)->value;
        auto group = OCIO::GroupTransform::Create();
        if (*look) {
            auto l = OCIO::LookTransform::Create();
            l->setSrc(source); l->setDst(source); l->setLooks(look);
            group->appendTransform(l);
        }
        if (*view) {
            auto t = OCIO::DisplayViewTransform::Create();
            t->setSrc(source); t->setDisplay(destination); t->setView(view);
            group->appendTransform(t);
        } else {
            auto t = OCIO::ColorSpaceTransform::Create();
            t->setSrc(source); t->setDst(destination);
            group->appendTransform(t);
        }
        auto processor = c->getProcessor(static_cast<Config *>(p)->context, group, OCIO::TRANSFORM_DIR_FORWARD);
        return new Processor{processor, processor->getDefaultCPUProcessor()};
    }, err, size);
}
// Optional GPU export extension to ABI 1. Descriptors contain owned JSON; no
// OCIO pointers escape. Texture/descriptor sizes are bounded before copying.
void *fold_ocio_gpu(void *p, char *err, size_t size) noexcept {
    return create([&]() -> void * {
        auto desc = OCIO::GpuShaderDesc::CreateShaderDesc();
        desc->setLanguage(OCIO::GPU_LANGUAGE_GLSL_4_0);
        desc->setFunctionName("fold_ocio");
        desc->setAllowTexture1D(false);
        desc->setTextureMaxWidth(4096);
        static_cast<Processor *>(p)->value->getDefaultGPUProcessor()->extractGpuShaderInfo(desc);
        if (desc->getNumUniforms()) throw std::runtime_error("dynamic OCIO uniforms require the CPU backend");
        if (desc->getNumTextures() + desc->getNum3DTextures() > 8)
            throw std::runtime_error("OCIO GPU texture count exceeds 8");
        auto quote = [](const char *text) {
            std::string out = "\"";
            for (const unsigned char c : std::string(text)) {
                if (c == '\n') out += "\\n";
                else if (c == '\r') out += "\\r";
                else if (c == '\t') out += "\\t";
                else if (c == '\\' || c == '"') { out += '\\'; out += c; }
                else if (c < 32) throw std::runtime_error("control byte in GPU descriptor");
                else out += c;
            }
            return out + "\"";
        };
        std::ostringstream out;
        out << std::setprecision(9) << "{\"shader\":" << quote(desc->getShaderText()) << ",\"textures\":[";
        size_t total = 0;
        unsigned index = 0;
        auto texture = [&](const char *sampler, unsigned w, unsigned h, unsigned d, unsigned channels,
                           OCIO::Interpolation interpolation, const float *values) {
            const size_t count = size_t(w) * h * d * channels;
            if (!w || !h || !d || w > 4096 || h > 4096 || d > 129 || count > 4*1024*1024 || total + count > 4*1024*1024)
                throw std::runtime_error("OCIO GPU LUT budget exceeded");
            total += count;
            if (index++) out << ',';
            out << "{\"sampler\":" << quote(sampler) << ",\"size\":[" << w << ',' << h << ',' << d
                << "],\"channels\":" << channels << ",\"linear\":" << (interpolation == OCIO::INTERP_LINEAR ? "true" : "false")
                << ",\"values\":[";
            for (size_t i = 0; i < count; ++i) {
                if (!std::isfinite(values[i])) throw std::runtime_error("nonfinite OCIO GPU LUT");
                if (i) out << ',';
                out << values[i];
            }
            out << "]}";
        };
        for (unsigned i = 0; i < desc->getNumTextures(); ++i) {
            const char *name, *sampler; unsigned w, h;
            OCIO::GpuShaderDesc::TextureType channels;
            OCIO::GpuShaderDesc::TextureDimensions dimensions;
            OCIO::Interpolation interpolation;
            desc->getTexture(i, name, sampler, w, h, channels, dimensions, interpolation);
            const float *values; desc->getTextureValues(i, values);
            texture(sampler, w, h, 1, channels == OCIO::GpuShaderDesc::TEXTURE_RED_CHANNEL ? 1 : 3, interpolation, values);
        }
        for (unsigned i = 0; i < desc->getNum3DTextures(); ++i) {
            const char *name, *sampler; unsigned edge; OCIO::Interpolation interpolation;
            desc->get3DTexture(i, name, sampler, edge, interpolation);
            const float *values; desc->get3DTextureValues(i, values);
            texture(sampler, edge, edge, edge, 3, interpolation, values);
        }
        out << "]}";
        return new std::string(out.str());
    }, err, size);
}
const char *fold_ocio_gpu_json(void *p) noexcept { return static_cast<std::string *>(p)->c_str(); }
void fold_ocio_gpu_drop(void *p) noexcept { delete static_cast<std::string *>(p); }
void fold_ocio_processor_drop(void *p) noexcept { delete static_cast<Processor *>(p); }
const char *fold_ocio_processor_id(void *p) noexcept {
    try { return static_cast<Processor *>(p)->cpu->getCacheID(); } catch (...) { return nullptr; }
}
int fold_ocio_files(void *p) noexcept {
    try { return static_cast<Processor *>(p)->value->getProcessorMetadata()->getNumFiles(); }
    catch (...) { return -1; }
}
// Resolve LUT names using this config's context, not the process working directory.
int fold_ocio_file(void *c, void *p, int index, char *out, size_t size) noexcept {
    try {
        auto file = static_cast<Processor *>(p)->value->getProcessorMetadata()->getFile(index);
        auto resolved = static_cast<Config *>(c)->context->resolveFileLocation(file);
        if (std::string(resolved).size() >= size) throw std::runtime_error("resource path too long");
        error(out, size, resolved);
        return 1;
    } catch (const std::exception &e) { error(out, size, e.what()); }
    catch (...) { error(out, size, "resource resolution failed"); }
    return 0;
}
int fold_ocio_apply(void *p, float *rgba, long pixels, char *err, size_t size) noexcept {
    try {
        // Color transforms operate on straight RGB only. OCIO's optimized
        // exponent may perturb even identity alpha (1 -> 1.0000116); never
        // expose coverage to a display/transfer processor.
        OCIO::PackedImageDesc image(rgba, pixels, 1, 3, OCIO::BIT_DEPTH_F32,
                                   sizeof(float), 4 * sizeof(float), pixels * 4 * sizeof(float));
        static_cast<Processor *>(p)->cpu->apply(image);
        return 1;
    } catch (const std::exception &e) { error(err, size, e.what()); }
    catch (...) { error(err, size, "OCIO processing failed"); }
    return 0;
}
}
