#pragma warning( disable: 4100 4103)

#include "deepkernel.h"
#include "DBKFunc.h"
#include <windef.h>

#include "vmxhelper.h"


BOOLEAN MakeWritableKM(PVOID StartAddress,UINT_PTR size)
{
#ifndef AMD64
	UINT_PTR PTE,PDE;
	struct PTEStruct *x;
	UINT_PTR CurrentAddress=(UINT_PTR)StartAddress;	

	while (CurrentAddress<((UINT_PTR)StartAddress+size))
	{
		
		PTE=(UINT_PTR)CurrentAddress;
		PTE=PTE/0x1000*PTESize+0xc0000000;

		PTE=(UINT_PTR)StartAddress;
		PTE=PTE/0x1000*PTESize+0xc0000000;

	    
	    PDE=PTE/0x1000*PTESize+0xc0000000; 

		x=(PVOID)PDE;
		if ((x->P==0) && (x->A2==0))
		{
			CurrentAddress+=PAGE_SIZE_LARGE;
			continue;
		}

		if (x->PS==1)
		{
			
			x->RW=1;
			CurrentAddress+=PAGE_SIZE_LARGE;
			continue;
		}

		CurrentAddress+=0x1000;
		x=(PVOID)PTE;
		if ((x->P==0) && (x->A2==0))
			continue; 
		x->RW=1;
	}

	return TRUE;
#else
	return FALSE;
#endif
}

BOOLEAN MakeWritable(PVOID StartAddress,UINT_PTR size,BOOLEAN usecopyonwrite)
{
#ifndef AMD64
	struct PTEStruct *x;
	unsigned char y;
	UINT_PTR CurrentAddress=(UINT_PTR)StartAddress;	

	
	if (((UINT_PTR)StartAddress>=0x80000000) || ((UINT_PTR)StartAddress+size>=0x80000000)) 
		return MakeWritableKM(StartAddress,size); 

	
	while (CurrentAddress<((UINT_PTR)StartAddress+size))
	{
		__try
		{
			y=*(PCHAR)CurrentAddress; 
			x=(PVOID)(CurrentAddress/0x1000*PTESize+0xc0000000);
			if (x->RW==0) 
			{
				if (usecopyonwrite)
                    x->A1=1;  
				else
					x->RW=1; 
			}
		}
		__except(1)
		{
			
		}	

        CurrentAddress+=0x1000;
	}	

	return TRUE;
#else
	return FALSE;
#endif
}


BOOLEAN CheckImageName(IN PUNICODE_STRING FullImageName, IN char* List,int listsize)
{
#ifndef AMD64
	
	ANSI_STRING tempstring;
	int i;

	DbgPrint("Checking this image name...\n");
	RtlZeroMemory(&tempstring,sizeof(ANSI_STRING));
	if (RtlUnicodeStringToAnsiString(&tempstring,FullImageName,TRUE)== STATUS_SUCCESS)
	{
		char *p;
		INT_PTR modulesize;
		__try
		{
			RtlUpperString(&tempstring,&tempstring);

			p=List;
	
			for (i=0;i<listsize;i++)
			{
				if (List[i]=='\0')
				{
					modulesize=i-(INT_PTR)(p-List);
					if (modulesize>=0)
					{	
						DbgPrint("Checking %s with %s\n",&tempstring.Buffer[tempstring.Length-modulesize],p);

						if ((tempstring.Length>=modulesize) && (strcmp(p,&tempstring.Buffer[tempstring.Length-modulesize])==0))
						{
														DbgPrint("It's a match with %s\n",p);
							return TRUE;	
						}						
	
					}
					p=&List[i+1];
				}
	
			}
		
			
		}
		__finally
		{
			RtlFreeAnsiString(&tempstring);	
		}
	}

	DbgPrint("No match\n");
#endif
	return FALSE;

}

VOID LoadImageNotifyRoutine(IN PUNICODE_STRING  FullImageName, IN HANDLE  ProcessId, IN PIMAGE_INFO  ImageInfo)
{
#ifndef AMD64
    
    char allowedModules[] = "notepad.exe\0calc.exe\0explorer.exe\0";
    int listSize = sizeof(allowedModules);
    
    DbgPrint("LoadImageNotify: PID=%d, Image=%wZ\n", ProcessId, FullImageName);
    
    
    if (CheckImageName(FullImageName, allowedModules, listSize))
    {
        DbgPrint("Allowed module loaded, applying memory permissions\n");
        
        
        if (ImageInfo && ImageInfo->ImageBase)
        {
            MakeWritable(ImageInfo->ImageBase, ImageInfo->ImageSize, TRUE);
        }
    }
    else
    {
        DbgPrint("Module not in allowed list, skipping\n");
    }
#else
    UNREFERENCED_PARAMETER(FullImageName);
    UNREFERENCED_PARAMETER(ProcessId);
    UNREFERENCED_PARAMETER(ImageInfo);
#endif
}


VOID DriverUnload(PDRIVER_OBJECT DriverObject)
{
    NTSTATUS status;
    
    DbgPrint("Unloading Polymarket Engine Kernel Manager\n");
    
    
    status = PsRemoveLoadImageNotifyRoutine(LoadImageNotifyRoutine);
    if (!NT_SUCCESS(status))
    {
        DbgPrint("Failed to remove load image notify routine: 0x%X\n", status);
    }
    
    DbgPrint("Polymarket Engine Kernel Manager Unloaded\n");
}


NTSTATUS DriverEntry(PDRIVER_OBJECT DriverObject, PUNICODE_STRING RegistryPath)
{
    NTSTATUS status;
    
    DbgPrint("Polymarket Engine Kernel Manager Loading...\n");
    
    DriverObject->DriverUnload = DriverUnload;
    
    
    status = PsSetLoadImageNotifyRoutine(LoadImageNotifyRoutine);
    if (!NT_SUCCESS(status))
    {
        DbgPrint("Failed to register load image notify routine: 0x%X\n", status);
        return status;
    }
    
    DbgPrint("Polymarket Engine Kernel Manager Loaded Successfully\n");
    return STATUS_SUCCESS;
}