# Backport https://github.com/jbeder/yaml-cpp/pull/1310 to OCIO's yaml-cpp 0.7.0.
# GCC 15 no longer supplies this header indirectly.
set(path "${SOURCE_DIR}/src/emitterutils.cpp")
file(READ "${path}" source)
if(NOT source MATCHES "#include <cstdint>")
    if(NOT source MATCHES "#include <algorithm>")
        message(FATAL_ERROR "Unexpected yaml-cpp source; review the cstdint backport")
    endif()
    string(REPLACE "#include <algorithm>" "#include <algorithm>\n#include <cstdint>" source "${source}")
    file(WRITE "${path}" "${source}")
endif()
