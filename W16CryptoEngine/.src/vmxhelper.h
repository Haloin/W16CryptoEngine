#ifndef VMXHELPER_H
#define VMXHELPER_H

#include <ntddk.h>
#include <wdm.h>

// Virtualization helper functions for VMX operations
#ifdef __cplusplus
extern "C" {
#endif

// VMX operation status codes
typedef enum _VMX_RESULT {
    VMX_SUCCESS = 0,
    VMX_ERROR_INVALID_VMCS = 1,
    VMX_ERROR_VMXON_IN_PROGRESS = 2,
    VMX_ERROR_INVALID_CONTROL_STATE = 3,
    VMX_ERROR_UNSUPPORTED_CPU = 4
} VMX_RESULT;

// Function prototypes
VMX_RESULT InitializeVMX();
VOID CleanupVMX();
BOOLEAN IsVMXSupported();
NTSTATUS EnableVMXOperation();
VOID DisableVMXOperation();

#ifdef __cplusplus
}
#endif

#endif // VMXHELPER_H
