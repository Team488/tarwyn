cmake_minimum_required(VERSION 3.20)
project(tarwyn_cpp LANGUAGES C CXX)

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

set(WPILIB_INCLUDE_DIR "" CACHE PATH "Directory holding the unpacked wpimath headers (wpi/math/...)")
set(TARWYN_C_TESTS ON)

if(NOT TARGET tarwyn::c)
    add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../c" tarwyn_c)
endif()

add_library(tarwyn INTERFACE)
add_library(tarwyn::tarwyn ALIAS tarwyn)
target_include_directories(tarwyn INTERFACE
    "${CMAKE_CURRENT_SOURCE_DIR}/include"
    "${WPILIB_INCLUDE_DIR}")
target_link_libraries(tarwyn INTERFACE tarwyn::c)
target_compile_features(tarwyn INTERFACE cxx_std_23)

enable_testing()
add_executable(offline_client tests/offline_client.cpp)
target_link_libraries(offline_client PRIVATE tarwyn::tarwyn)
target_compile_options(offline_client PRIVATE
    $<$<CXX_COMPILER_ID:GNU,Clang,AppleClang>:-Wall -Wextra -Werror>)
add_test(NAME offline_client COMMAND offline_client)
