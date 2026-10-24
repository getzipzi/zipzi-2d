cmake_minimum_required(VERSION 3.21)
project(zipzi_probes LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(ZIPZI_CUBISM_ROOT "" CACHE PATH "Local Cubism Native SDK root")

# Build PurismCore submodule reference
add_subdirectory(${CMAKE_CURRENT_SOURCE_DIR}/../../../third_party/purism-core ${CMAKE_CURRENT_BINARY_DIR}/purism)

# 1. Purism Probe
add_executable(zipzi_document_purism_probe project_runtime_probe.cpp)
target_link_libraries(zipzi_document_purism_probe PRIVATE PurismCore)

# 2. Official Core Probe (if Cubism SDK root provided)
if(ZIPZI_CUBISM_ROOT)
  if(APPLE)
    set(_cubism_platform macos)
  elseif(UNIX)
    set(_cubism_platform linux)
  elseif(WIN32)
    set(_cubism_platform windows)
  else()
    message(FATAL_ERROR "Unsupported platform for official Core")
  endif()

  set(_cubism_library "${ZIPZI_CUBISM_ROOT}/Core/lib/${_cubism_platform}/${CMAKE_SYSTEM_PROCESSOR}/libLive2DCubismCore.a")
  if(NOT EXISTS "${_cubism_library}")
    message(FATAL_ERROR "Missing official Core: ${_cubism_library}")
  endif()

  add_executable(zipzi_document_official_probe project_runtime_probe.cpp)
  target_compile_definitions(zipzi_document_official_probe PRIVATE ZIPZI_OFFICIAL_CORE)
  target_include_directories(zipzi_document_official_probe PRIVATE "${ZIPZI_CUBISM_ROOT}/Core/include")
  target_link_libraries(zipzi_document_official_probe PRIVATE "${_cubism_library}")

  # CPU-only animation oracle. Keep the source list explicit so the probe does
  # not depend on a graphics backend or silently pick up SDK build products.
  set(_framework_src "${ZIPZI_CUBISM_ROOT}/Framework/src")
  add_library(zipzi_framework_cpu STATIC
    ${_framework_src}/CubismFramework.cpp
    ${_framework_src}/CubismCdiJson.cpp
    ${_framework_src}/Id/CubismId.cpp
    ${_framework_src}/Id/CubismIdManager.cpp
    ${_framework_src}/Math/CubismMath.cpp
    ${_framework_src}/Math/CubismMatrix44.cpp
    ${_framework_src}/Math/CubismVector2.cpp
    ${_framework_src}/Model/CubismMoc.cpp
    ${_framework_src}/Model/CubismModel.cpp
    ${_framework_src}/Model/CubismModelMultiplyAndScreenColor.cpp
    ${_framework_src}/Motion/ACubismMotion.cpp
    ${_framework_src}/Motion/CubismExpressionMotion.cpp
    ${_framework_src}/Motion/CubismExpressionMotionManager.cpp
    ${_framework_src}/Motion/CubismMotion.cpp
    ${_framework_src}/Motion/CubismMotionJson.cpp
    ${_framework_src}/Motion/CubismMotionManager.cpp
    ${_framework_src}/Motion/CubismMotionQueueEntry.cpp
    ${_framework_src}/Motion/CubismMotionQueueManager.cpp
    ${_framework_src}/Effect/CubismPose.cpp
    ${_framework_src}/Physics/CubismPhysics.cpp
    ${_framework_src}/Physics/CubismPhysicsJson.cpp
    ${_framework_src}/CubismModelSettingJson.cpp
    ${_framework_src}/Rendering/CubismRenderer.cpp
    ${_framework_src}/Rendering/csmBlendMode.cpp
    ${_framework_src}/Type/csmString.cpp
    ${_framework_src}/Utils/CubismDebug.cpp
    ${_framework_src}/Utils/CubismJson.cpp
    ${_framework_src}/Utils/CubismString.cpp)
  target_include_directories(zipzi_framework_cpu PUBLIC "${_framework_src}" "${ZIPZI_CUBISM_ROOT}/Core/include")
  target_link_libraries(zipzi_framework_cpu PUBLIC "${_cubism_library}")
  add_executable(zipzi_framework_animation_cpu_probe
    framework_animation_cpu_probe.cpp framework_cpu_renderer_cleanup.cpp)
  target_link_libraries(zipzi_framework_animation_cpu_probe PRIVATE zipzi_framework_cpu)
  add_executable(zipzi_framework_cdi_cpu_probe
    framework_cdi_cpu_probe.cpp framework_cpu_renderer_cleanup.cpp)
  target_link_libraries(zipzi_framework_cdi_cpu_probe PRIVATE zipzi_framework_cpu)
endif()

option(ZIPZI_BUILD_GPU_PROBE "Build the official Framework OpenGL image oracle" OFF)
if(ZIPZI_BUILD_GPU_PROBE)
  if(NOT APPLE OR NOT ZIPZI_CUBISM_ROOT)
    message(FATAL_ERROR "GPU probe currently requires macOS and a Native SDK")
  endif()
  set(_third_party "${ZIPZI_CUBISM_ROOT}/Samples/OpenGL/thirdParty")
  set(GLFW_BUILD_DOCS OFF CACHE BOOL "" FORCE)
  set(GLFW_BUILD_TESTS OFF CACHE BOOL "" FORCE)
  set(GLFW_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
  set(GLFW_INSTALL OFF CACHE BOOL "" FORCE)
  set(BUILD_UTILS OFF CACHE BOOL "" FORCE)
  add_subdirectory(${_third_party}/glew/build/cmake ${CMAKE_CURRENT_BINARY_DIR}/glew)
  add_subdirectory(${_third_party}/glfw ${CMAKE_CURRENT_BINARY_DIR}/glfw)
  set(FRAMEWORK_SOURCE OpenGL)
  add_subdirectory(${ZIPZI_CUBISM_ROOT}/Framework ${CMAKE_CURRENT_BINARY_DIR}/Framework)
  target_compile_definitions(Framework PUBLIC CSM_TARGET_MAC_GL)
  target_include_directories(Framework PUBLIC ${_third_party}/glew/include ${ZIPZI_CUBISM_ROOT}/Core/include)
  target_link_libraries(Framework PUBLIC glew_s "${_cubism_library}")
  find_package(OpenGL REQUIRED)
  find_package(ZLIB REQUIRED)
  add_executable(zipzi_framework_gpu_probe framework_gpu_probe.cpp)
  target_include_directories(zipzi_framework_gpu_probe PRIVATE ${_third_party}/stb)
  target_link_libraries(zipzi_framework_gpu_probe PRIVATE Framework glfw ${OPENGL_LIBRARIES} ZLIB::ZLIB)
endif()
