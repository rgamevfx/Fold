// Private ABI v1. Never propagate a C++ exception or expose OCIO's C++ ABI to Rust.
#include <OpenColorIO/OpenColorIO.h>
#include <cstdio>
#include <memory>
#include <stdexcept>
#include <string>
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
