cmake_minimum_required(VERSION 3.10)

# Force dynamic CRT (/MD) on MSVC before project() initializes flags.
# CMP0091 enables CMAKE_MSVC_RUNTIME_LIBRARY; prime the cache entry so the
# setting survives the project() call below.
if(POLICY CMP0091)
  cmake_policy(SET CMP0091 NEW)
endif()
if(MSVC)
  set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreadedDLL"
      CACHE STRING "MSVC runtime library" FORCE)
endif()

project(knfc)

# After project() initializes flags, replace any /MT or /MTd with /MD or /MDd.
# This is a belt-and-suspenders fallback that works on all cmake versions.
if(MSVC)
  foreach(_flag_var
      CMAKE_C_FLAGS CMAKE_C_FLAGS_DEBUG CMAKE_C_FLAGS_RELEASE
      CMAKE_C_FLAGS_MINSIZEREL CMAKE_C_FLAGS_RELWITHDEBINFO
      CMAKE_CXX_FLAGS CMAKE_CXX_FLAGS_DEBUG CMAKE_CXX_FLAGS_RELEASE
      CMAKE_CXX_FLAGS_MINSIZEREL CMAKE_CXX_FLAGS_RELWITHDEBINFO)
    if(DEFINED ${_flag_var})
      string(REPLACE "/MT " "/MD " ${_flag_var} "${${_flag_var}}")
      string(REPLACE "/MT\n" "/MD\n" ${_flag_var} "${${_flag_var}}")
      string(REPLACE "/MTd " "/MDd " ${_flag_var} "${${_flag_var}}")
      string(REPLACE "/MTd\n" "/MDd\n" ${_flag_var} "${${_flag_var}}")
      # Handle /MT at end-of-string
      string(REGEX REPLACE "/MT$" "/MD" ${_flag_var} "${${_flag_var}}")
      string(REGEX REPLACE "/MTd$" "/MDd" ${_flag_var} "${${_flag_var}}")
      set(${_flag_var} "${${_flag_var}}" CACHE STRING "" FORCE)
    endif()
  endforeach()
endif()

set(CMAKE_CXX_STANDARD 14)  # Change this from 11 to 14
set(CMAKE_CXX_STANDARD_REQUIRED True)

# Define options for the external project

set(BUILD_SHARED_LIBS OFF CACHE BOOL "Disable shared lib")
set(KALDI_NATIVE_FBANK_BUILD_PYTHON OFF CACHE BOOL "Disable Python build")
set(KALDI_NATIVE_FBANK_BUILD_TESTS OFF CACHE BOOL "Disable Tests build")

add_subdirectory(../knf ${CMAKE_BINARY_DIR}/knf)

add_library(knfc STATIC knfc.cc)

target_link_libraries(knfc PRIVATE kaldi-native-fbank-core)

target_include_directories(knfc PRIVATE
    ${KNF_LIB_PATH}/include
    ../knf/kaldi-native-fbank/csrc
    ../knf
)

install(TARGETS knfc
        ARCHIVE DESTINATION lib)