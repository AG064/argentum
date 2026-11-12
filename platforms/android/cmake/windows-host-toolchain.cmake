if(NOT DEFINED ENV{VCToolsInstallDir})
    message(FATAL_ERROR "VCToolsInstallDir is missing; run the Android build from a Visual Studio developer environment")
endif()

if(NOT DEFINED ENV{WindowsSdkDir} OR NOT DEFINED ENV{WindowsSDKVersion})
    message(FATAL_ERROR "Windows SDK environment is missing; run the Android build from a Visual Studio developer environment")
endif()

file(TO_CMAKE_PATH "$ENV{VCToolsInstallDir}" ARGENTUM_VC_TOOLS)
file(TO_CMAKE_PATH "$ENV{WindowsSdkDir}" ARGENTUM_WINDOWS_SDK)
string(REGEX REPLACE "[/\\\\]+$" "" ARGENTUM_WINDOWS_SDK_VERSION "$ENV{WindowsSDKVersion}")

set(CMAKE_C_COMPILER "${ARGENTUM_VC_TOOLS}/bin/Hostx64/x64/cl.exe")
set(CMAKE_CXX_COMPILER "${ARGENTUM_VC_TOOLS}/bin/Hostx64/x64/cl.exe")
set(CMAKE_LINKER "${ARGENTUM_VC_TOOLS}/bin/Hostx64/x64/link.exe")
set(CMAKE_RC_COMPILER "${ARGENTUM_WINDOWS_SDK}/bin/${ARGENTUM_WINDOWS_SDK_VERSION}/x64/rc.exe")
set(CMAKE_MT "${ARGENTUM_WINDOWS_SDK}/bin/${ARGENTUM_WINDOWS_SDK_VERSION}/x64/mt.exe")
