#ifndef DEEPKERNEL_H
#define DEEPKERNEL_H

#include <ntddk.h>
#include <wdm.h>


struct PTEStruct {
    UCHAR P;      
    UCHAR RW;
    UCHAR A1;     
    UCHAR A2;     
    UCHAR PS;     
    
};

#define PAGE_SIZE_LARGE 0x200000  
extern ULONG PTESize;


BOOLEAN MakeWritableKM(PVOID StartAddress, UINT_PTR size);
BOOLEAN MakeWritable(PVOID StartAddress, UINT_PTR size, BOOLEAN usecopyonwrite);
BOOLEAN CheckImageName(IN PUNICODE_STRING FullImageName, IN char* List, int listsize);

#endif 
