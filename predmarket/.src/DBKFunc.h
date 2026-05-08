#ifndef DBKFUNC_H
#define DBKFUNC_H

#include <ntddk.h>
#include <wdm.h>


extern BOOLEAN usecopyonwrite;


NTSTATUS InitializeDatabaseKernel();
VOID CleanupDatabaseKernel();


NTSTATUS AllocateKernelMemory(PVOID* Address, SIZE_T Size);
VOID FreeKernelMemory(PVOID Address);


NTSTATUS InitializeProcessMonitor();
VOID CleanupProcessMonitor();

#endif 
