cmake_minimum_required(VERSION 3.20)
project(ctx NONE)

include(ExternalProject)

set(CTX_CARGO_TARGET_DIR "${CMAKE_BINARY_DIR}/cargo")
set(CTX_PROFILE release)

# Auto-detect architecture, mirroring scripts/crossgen.sh / toolchain.cmake
execute_process(COMMAND uname -m OUTPUT_VARIABLE CTX_ARCH OUTPUT_STRIP_TRAILING_WHITESPACE)
if(CTX_ARCH STREQUAL "x86_64")
  set(CTX_RUST_TARGET x86_64-unknown-linux-musl)
elseif(CTX_ARCH STREQUAL "aarch64")
  set(CTX_RUST_TARGET aarch64-unknown-linux-musl)
else()
  message(FATAL_ERROR "Unsupported architecture: ${CTX_ARCH}. Supported: x86_64, aarch64")
endif()

set(CTX_BINARY "${CTX_CARGO_TARGET_DIR}/${CTX_RUST_TARGET}/${CTX_PROFILE}/ctx")

ExternalProject_Add(ctx_build
  SOURCE_DIR "${CMAKE_CURRENT_SOURCE_DIR}"
  BUILD_IN_SOURCE TRUE
  CONFIGURE_COMMAND ""
  BUILD_COMMAND env CARGO_TARGET_DIR=${CTX_CARGO_TARGET_DIR}
    cargo build --profile ${CTX_PROFILE} --locked --target ${CTX_RUST_TARGET}
  BUILD_ALWAYS TRUE
  INSTALL_COMMAND ""
  BUILD_BYPRODUCTS "${CTX_BINARY}"
)

install(FILES "${CTX_BINARY}"
  DESTINATION "${CMAKE_INSTALL_PREFIX}/bin"
  PERMISSIONS OWNER_READ OWNER_WRITE OWNER_EXECUTE GROUP_READ GROUP_EXECUTE WORLD_READ WORLD_EXECUTE
)
install(FILES docs/ctx.1
  DESTINATION "${CMAKE_INSTALL_PREFIX}/share/man/man1"
)
