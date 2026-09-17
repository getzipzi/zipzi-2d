cmake_minimum_required(VERSION 3.16)

# Set app name.
set(APP_NAME Demo)
# Set directory paths.
set(
  SDK_ROOT_PATH
  "${CMAKE_CURRENT_LIST_DIR}/../../../../third_party/CubismSdkForNative-5-r.5"
  CACHE PATH
  "Cubism Native SDK root"
)
set(CORE_PROVIDER "cubism" CACHE STRING "Core implementation: cubism or purism")
set_property(CACHE CORE_PROVIDER PROPERTY STRINGS cubism purism)
set(
  PURISM_CORE_LIBRARY
  "${CMAKE_CURRENT_LIST_DIR}/../../../../target/cubism-matrix/core/purism-v6/libPurismCore.a"
  CACHE FILEPATH
  "Purism Core v6 static library"
)
set(BENCHMARK_CASE_ID "${CORE_PROVIDER}-native" CACHE STRING "Benchmark case identifier")
set(BENCHMARK_WORKLOAD_ID "mao-40" CACHE STRING "Benchmark workload identifier")
set(BENCHMARK_MODEL_NAME "mao_pro" CACHE STRING "Model identifier")
set(BENCHMARK_MODEL_HASH "unknown" CACHE STRING "SHA-256 of the model3 fixture")
set(BENCHMARK_MODEL_COUNT 40 CACHE STRING "Number of model instances")
set(BENCHMARK_COLUMNS 10 CACHE STRING "Grid columns")
set(BENCHMARK_ROWS 4 CACHE STRING "Grid rows")
set(BENCHMARK_WIDTH 1280 CACHE STRING "Framebuffer width")
set(BENCHMARK_HEIGHT 720 CACHE STRING "Framebuffer height")
set(BENCHMARK_WARMUP_SECONDS 5.0 CACHE STRING "Warmup duration")
set(BENCHMARK_SAMPLE_SECONDS 15.0 CACHE STRING "Sample duration")
set(CORE_PATH ${SDK_ROOT_PATH}/Core)
set(FRAMEWORK_PATH ${SDK_ROOT_PATH}/Framework)
set(COMMON_SRC_PATH ${SDK_ROOT_PATH}/Samples/Common)
set(THIRD_PARTY_PATH ${SDK_ROOT_PATH}/Samples/OpenGL/thirdParty)
set(STB_PATH ${THIRD_PARTY_PATH}/stb)
set(GLEW_PATH ${THIRD_PARTY_PATH}/glew)
set(GLFW_PATH ${THIRD_PARTY_PATH}/glfw)
set(MODEL_RES_PATH ${CMAKE_CURRENT_SOURCE_DIR}/../../assets/live2d/mao/runtime)
set(SAMPLE_SHADER_PATH ${SDK_ROOT_PATH}/Samples/OpenGL/Shaders/Standard)
set(FRAMEWORK_SHADER_PATH ${FRAMEWORK_PATH}/src/Rendering/OpenGL/Shaders/Standard)

# Set project.
project(${APP_NAME})

# Define output directory.
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/bin/${APP_NAME})
# Set configuration (Release and Debug only).
set(CMAKE_CONFIGURATION_TYPES Debug Release
  CACHE STRING "Configurations" FORCE
)
# Suppress generation of ZERO_CHECK project.
set(CMAKE_SUPPRESS_REGENERATION ON)

# Surpress GLEW and GLFW process.
set(GLFW_BUILD_DOCS OFF CACHE BOOL "" FORCE)
set(GLFW_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(GLFW_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(GLFW_INSTALL OFF CACHE BOOL "" FORCE)
set(BUILD_UTILS OFF CACHE BOOL "" FORCE)

# Specify version of compiler.
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Both providers expose the Cubism Core C ABI and use the official headers.
add_library(Live2DCubismCore STATIC IMPORTED)
# Get architecture.
EXECUTE_PROCESS( COMMAND uname -m COMMAND tr -d '\n' OUTPUT_VARIABLE ARCHITECTURE )
# Set library path and inlude path.
if (CORE_PROVIDER STREQUAL "cubism")
  set(CORE_LIBRARY ${CORE_PATH}/lib/macos/${ARCHITECTURE}/libLive2DCubismCore.a)
elseif (CORE_PROVIDER STREQUAL "purism")
  set(CORE_LIBRARY ${PURISM_CORE_LIBRARY})
else ()
  message(FATAL_ERROR "CORE_PROVIDER must be 'cubism' or 'purism'")
endif ()
if (NOT EXISTS ${CORE_LIBRARY})
  message(FATAL_ERROR "Core library does not exist: ${CORE_LIBRARY}")
endif ()
set_target_properties(Live2DCubismCore PROPERTIES
  IMPORTED_LOCATION ${CORE_LIBRARY}
  INTERFACE_INCLUDE_DIRECTORIES ${CORE_PATH}/include
)

# Add GLEW ,GLFW.
add_subdirectory(${GLEW_PATH}/build/cmake ${CMAKE_CURRENT_BINARY_DIR}/glew)
add_subdirectory(${GLFW_PATH} ${CMAKE_CURRENT_BINARY_DIR}/glfw)

# Specify Cubism Framework rendering.
set(FRAMEWORK_SOURCE OpenGL)
# Add Cubism Native Framework.
add_subdirectory(${FRAMEWORK_PATH} ${CMAKE_CURRENT_BINARY_DIR}/Framework)
# Add rendering definition to framework.
target_compile_definitions(Framework PUBLIC CSM_TARGET_MAC_GL)
# Add include path of GLEW to framework.
target_include_directories(Framework PUBLIC ${GLEW_PATH}/include)
# Link libraries to framework.
target_link_libraries(Framework Live2DCubismCore glew_s)

# Find opengl libraries.
find_package(OpenGL REQUIRED)

# Make executable app.
add_executable(${APP_NAME})
target_compile_definitions(${APP_NAME} PRIVATE
  KASANE_BENCHMARK_CASE_ID="${BENCHMARK_CASE_ID}"
  KASANE_BENCHMARK_CORE_PROVIDER="${CORE_PROVIDER}"
  KASANE_BENCHMARK_WORKLOAD_ID="${BENCHMARK_WORKLOAD_ID}"
  KASANE_BENCHMARK_MODEL_NAME="${BENCHMARK_MODEL_NAME}"
  KASANE_BENCHMARK_MODEL_HASH="${BENCHMARK_MODEL_HASH}"
  KASANE_BENCHMARK_MODEL_COUNT=${BENCHMARK_MODEL_COUNT}
  KASANE_BENCHMARK_COLUMNS=${BENCHMARK_COLUMNS}
  KASANE_BENCHMARK_ROWS=${BENCHMARK_ROWS}
  KASANE_BENCHMARK_WIDTH=${BENCHMARK_WIDTH}
  KASANE_BENCHMARK_HEIGHT=${BENCHMARK_HEIGHT}
  KASANE_BENCHMARK_WARMUP_SECONDS=${BENCHMARK_WARMUP_SECONDS}
  KASANE_BENCHMARK_SAMPLE_SECONDS=${BENCHMARK_SAMPLE_SECONDS}
)
# Add common source files.
add_subdirectory(${COMMON_SRC_PATH} ${CMAKE_CURRENT_BINARY_DIR}/src/Common)
# Add source files.
add_subdirectory(src)

# Link libraries to app.
target_link_libraries(${APP_NAME}
  Framework
  glfw
  ${OPENGL_LIBRARIES}
)
# Specify include directories.
target_include_directories(${APP_NAME} PRIVATE ${STB_PATH})

# Copy resource directory to build directory.
add_custom_command(
  TARGET ${APP_NAME}
  POST_BUILD
  COMMAND
    ${CMAKE_COMMAND} -E
      make_directory $<TARGET_FILE_DIR:${APP_NAME}>/Resources/Mao
  COMMAND
    ${CMAKE_COMMAND} -E
      copy_directory ${MODEL_RES_PATH} $<TARGET_FILE_DIR:${APP_NAME}>/Resources/Mao
  COMMAND
    ${CMAKE_COMMAND} -E
      copy_if_different
        ${MODEL_RES_PATH}/mao_pro.model3.json
        $<TARGET_FILE_DIR:${APP_NAME}>/Resources/Mao/Mao.model3.json
  COMMAND
    ${CMAKE_COMMAND} -E
      copy_directory ${SAMPLE_SHADER_PATH} $<TARGET_FILE_DIR:${APP_NAME}>/SampleShaders
  COMMAND
    ${CMAKE_COMMAND} -E
      copy_directory ${FRAMEWORK_SHADER_PATH} $<TARGET_FILE_DIR:${APP_NAME}>/FrameworkShaders
)

# You can change target that renderer draws by enabling following definition.
#
# * USE_RENDER_TARGET
#   Renderer draws to target of LAppView.
# * USE_MODEL_RENDER_TARGET
#   Renderer draws to target of each LAppModel.
# * default
#   Renderer draws to default main framebuffer.
#
# INFO: USE_RENDER_TARGET has higher priority than USE_MODEL_RENDER_TARGET.
#
# target_compile_definitions(${APP_NAME}
#   PRIVATE
#     USE_RENDER_TARGET
#     USE_MODEL_RENDER_TARGET
# )
