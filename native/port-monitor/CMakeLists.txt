cmake_minimum_required(VERSION 3.24)
project(StampaCPMonitor LANGUAGES CXX)

if(NOT WIN32 OR NOT CMAKE_SIZEOF_VOID_P EQUAL 8)
  message(FATAL_ERROR "StampaCP Port Monitor deve essere compilato per Windows x64.")
endif()

add_library(StampaCPMonitor SHARED port_monitor.cpp)
target_compile_features(StampaCPMonitor PRIVATE cxx_std_17)
target_compile_definitions(StampaCPMonitor PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
target_link_libraries(StampaCPMonitor PRIVATE Advapi32 Userenv Wtsapi32)
set_target_properties(StampaCPMonitor PROPERTIES
  OUTPUT_NAME "StampaCPMonitor"
  WINDOWS_EXPORT_ALL_SYMBOLS OFF
)
