cmake_minimum_required(VERSION 3.24)
project(CodVLegacyRoot LANGUAGES CXX)
# At the top level too, so ctest --test-dir build/host finds host/tests.
enable_testing()
add_subdirectory(host)
