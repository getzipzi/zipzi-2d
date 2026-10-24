cmake_minimum_required(VERSION 3.21)
project(zipzi_workspace LANGUAGES C CXX)
include(CTest)

option(ZIPZI_ENABLE_SANITIZERS "Instrument native targets with ASan and UBSan" OFF)
if(ZIPZI_ENABLE_SANITIZERS)
  if(MSVC)
    message(FATAL_ERROR "This sanitizer preset requires Clang or GCC")
  endif()
  add_compile_options(-fsanitize=address,undefined -fno-omit-frame-pointer)
  add_link_options(-fsanitize=address,undefined)
endif()

add_subdirectory(tools/validation/probes)
