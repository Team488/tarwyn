cmake_minimum_required(VERSION 3.20)
project(tarwyn_c LANGUAGES C)

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

set(TARWYN_LIBRARY_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../target/release"
    CACHE PATH "Directory holding the tarwyn shared library that cargo build -p tarwyn-c --release produced")

find_library(TARWYN_LIBRARY NAMES tarwyn PATHS "${TARWYN_LIBRARY_DIR}" NO_DEFAULT_PATH REQUIRED)

add_library(tarwyn_c INTERFACE)
add_library(tarwyn::c ALIAS tarwyn_c)
target_include_directories(tarwyn_c INTERFACE "${CMAKE_CURRENT_SOURCE_DIR}/include")
target_link_libraries(tarwyn_c INTERFACE "${TARWYN_LIBRARY}")
target_compile_features(tarwyn_c INTERFACE c_std_99)

if(PROJECT_IS_TOP_LEVEL OR TARWYN_C_TESTS)
    enable_testing()
    add_executable(abi_client tests/abi_client.c)
    target_link_libraries(abi_client PRIVATE tarwyn::c)
    target_compile_options(abi_client PRIVATE
        $<$<C_COMPILER_ID:GNU,Clang,AppleClang>:-Wall -Wextra -Werror>)
    add_test(NAME abi_client COMMAND abi_client)
endif()
