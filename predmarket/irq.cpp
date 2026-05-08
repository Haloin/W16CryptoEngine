#pragma warning( disable: 4103)

#include "ntifs.h"
#include <windef.h>

#include "DBKFunc.h"
#include "vmxhelper.h"

#include "interruptHook.h"
asm(
    ;RCX: 1st integer argument
;RDX: 2nd integer argument
;R8: 3rd integer argument
;R9: 4th integer argument

CALLBACK        struct
A		 		qword ?
S				qword ?
CALLBACK        ends


ASMENTRY_STACK	struct ;keep this 16 byte aligned
	Scratchspace	qword ?
	Scratchspace2	qword ? 
	Scratchspace3	qword ?
	Scratchspace4	qword ? 	
	Originalmxcsr	qword ?	
	OriginalRAX		qword ?  ;0
	OriginalRBX		qword ?  ;1
	OriginalRCX		qword ?  ;2
	OriginalRDX		qword ?  ;3
	OriginalRSI		qword ?  ;4
	OriginalRDI		qword ?  ;5
	OriginalRBP		qword ?  ;6
	OriginalRSP		qword ?  ;7 not really 'original'
	OriginalR8		qword ?  ;8
	OriginalR9		qword ?  ;9
	OriginalR10		qword ?  ;10
	OriginalR11		qword ?  ;11
	OriginalR12		qword ?  ;12
	OriginalR13		qword ?  ;13
	OriginalR14		qword ?  ;14
	OriginalR15		qword ?  ;15
	OriginalES		qword ?  ;16
	OriginalDS		qword ?	 ;17
	OriginalSS		qword ?	 ;18
	fxsavespace     db 512 dup(?)  ;fpu state

	;errorcode/returnaddress   ;19
	;4096 bytes 
	;eip     ;20
	;cs      ;21
	;eflags
	;esp
	;ss
	
ASMENTRY_STACK	ends


_TEXT SEGMENT 'CODE'

EXTERN interrupt1_centry : proc
EXTERN Int1JumpBackLocation : CALLBACK

PUBLIC interrupt1_asmentry
interrupt1_asmentry:
		;save stack position
		push [Int1JumpBackLocation.A] ;push an errorcode on the stack so the stackindex enum type can stay the same relative to interrupts that do have an errorcode (int 14).  Also helps with variable interrupt handlers
		
		sub rsp,4096  ;functions like setThreadContext adjust the stackframe entry directly. I can't have that messing up my own stack

		cld			

		;stack is aligned at this point
		sub rsp,SIZEOF ASMENTRY_STACK
		
		
		mov (ASMENTRY_STACK PTR [rsp]).OriginalRBP,rbp
		lea rbp,(ASMENTRY_STACK PTR [rsp]).OriginalRAX
		
		
		mov (ASMENTRY_STACK PTR [rsp]).OriginalRAX,rax	
		mov (ASMENTRY_STACK PTR [rsp]).OriginalRBX,rbx
		mov (ASMENTRY_STACK PTR [rsp]).OriginalRCX,rcx
		mov (ASMENTRY_STACK PTR [rsp]).OriginalRDX,rdx
		mov (ASMENTRY_STACK PTR [rsp]).OriginalRSI,rsi
		mov (ASMENTRY_STACK PTR [rsp]).OriginalRDI,rdi
		mov (ASMENTRY_STACK PTR [rsp]).OriginalRSP,rsp
		mov (ASMENTRY_STACK PTR [rsp]).OriginalR8,r8
		mov (ASMENTRY_STACK PTR [rsp]).OriginalR9,r9
		mov (ASMENTRY_STACK PTR [rsp]).OriginalR10,r10
		mov (ASMENTRY_STACK PTR [rsp]).OriginalR11,r11
		mov (ASMENTRY_STACK PTR [rsp]).OriginalR12,r12
		mov (ASMENTRY_STACK PTR [rsp]).OriginalR13,r13
		mov (ASMENTRY_STACK PTR [rsp]).OriginalR14,r14
		mov (ASMENTRY_STACK PTR [rsp]).OriginalR15,r15

		fxsave (ASMENTRY_STACK PTR [rsp]).fxsavespace

	
		
		
		
		mov ax,ds
		mov word ptr (ASMENTRY_STACK PTR [rsp]).OriginalDS,ax
		
		mov ax,es
		mov word ptr (ASMENTRY_STACK PTR [rsp]).OriginalES,ax
		
		mov ax,ss
		mov word ptr (ASMENTRY_STACK PTR [rsp]).OriginalSS,ax		
		

		mov ax,2bh 
		mov ds,ax
		mov es,ax
		
		mov ax,18h
		mov ss,ax
		
		
		; rbp= pointer to OriginalRAX
		
		cmp qword ptr [rbp+8*21+512+4096],010h ;check if origin is in kernelmode (check ss)
		je skipswap1 ;if so, skip the swapgs
		
		swapgs ;swap gs with the kernel version
		
skipswap1:
		
		
		
		stmxcsr dword ptr (ASMENTRY_STACK PTR [rsp]).Originalmxcsr
		
		mov (ASMENTRY_STACK PTR [rsp]).scratchspace2,1f80h		
		ldmxcsr dword ptr (ASMENTRY_STACK PTR [rsp]).scratchspace2

		
		mov rcx,rbp
		call interrupt1_centry
		
		ldmxcsr dword ptr (ASMENTRY_STACK PTR [rsp]).Originalmxcsr
		
		cmp qword ptr [rbp+8*21+512+4096],10h ;was it a kernelmode interrupt ?
		je skipswap2 ;if so, skip the swapgs part
				
		swapgs ;swap back
skipswap2:

		cmp al,1


		;restore state
		fxrstor (ASMENTRY_STACK PTR [rsp]).fxsavespace

		mov ax,word ptr (ASMENTRY_STACK PTR [rsp]).OriginalDS
		mov ds,ax
		
		mov ax,word ptr (ASMENTRY_STACK PTR [rsp]).OriginalES
		mov es,ax
		
		mov ax,word ptr (ASMENTRY_STACK PTR [rsp]).OriginalSS
		mov ss,ax
		
		
		mov rax,(ASMENTRY_STACK PTR [rsp]).OriginalRAX
		mov rbx,(ASMENTRY_STACK PTR [rsp]).OriginalRBX
		mov rcx,(ASMENTRY_STACK PTR [rsp]).OriginalRCX
		mov rdx,(ASMENTRY_STACK PTR [rsp]).OriginalRDX
		mov rsi,(ASMENTRY_STACK PTR [rsp]).OriginalRSI
		mov rdi,(ASMENTRY_STACK PTR [rsp]).OriginalRDI
		mov r8, (ASMENTRY_STACK PTR [rsp]).OriginalR8
		mov r9, (ASMENTRY_STACK PTR [rsp]).OriginalR9
		mov r10,(ASMENTRY_STACK PTR [rsp]).OriginalR10
		mov r11,(ASMENTRY_STACK PTR [rsp]).OriginalR11
		mov r12,(ASMENTRY_STACK PTR [rsp]).OriginalR12
		mov r13,(ASMENTRY_STACK PTR [rsp]).OriginalR13
		mov r14,(ASMENTRY_STACK PTR [rsp]).OriginalR14
		mov r15,(ASMENTRY_STACK PTR [rsp]).OriginalR15

	
		je skip_original_int1
		
		;stack unwind
		mov rbp,(ASMENTRY_STACK PTR [rsp]).OriginalRBP
		add rsp,SIZEOF ASMENTRY_STACK  
		add rsp,4096

		;at this point [rsp] holds the original int1 handler
		ret ; used to be add rsp,8 ;+8 for the push 0

		;todo: do a jmp [Int1JumpBackLocationCPUNR] and have 256 Int1JumpBackLocationCPUNR's and each cpu goes to it's own interrupt1_asmentry[cpunr]
		
		;jmp [Int1JumpBackLocation.A] ;<-works fine	


skip_original_int1:
		;stack unwind
		mov rbp,(ASMENTRY_STACK PTR [rsp]).OriginalRBP
		add rsp,SIZEOF ASMENTRY_STACK 	
		add rsp,4096
		add rsp,8  ;+8 for the push	
		
		iretq

	
_TEXT   ENDS
        END	
);

struct 
{
	int hooked;
	int dbvmInterruptEmulation;
	WORD originalCS;
	ULONG_PTR originalEIP;
} InterruptHook[256]; 


WORD inthook_getOriginalCS(unsigned char intnr)
{
	return InterruptHook[intnr].originalCS;
}

ULONG_PTR inthook_getOriginalEIP(unsigned char intnr)
{
	return InterruptHook[intnr].originalEIP;
}

int inthook_isHooked(unsigned char intnr)
{
	if (InterruptHook[intnr].hooked)
	{
	
		return TRUE;
	} else return FALSE;
}

int inthook_isDBVMHook(unsigned char intnr)
{
	return InterruptHook[intnr].dbvmInterruptEmulation;
}

int inthook_UnhookInterrupt(unsigned char intnr)
{
	if (InterruptHook[intnr].hooked)
	{
		
		DbgPrint("cpu %d : interrupt %d is hooked\n",cpunr(),intnr);
		if (InterruptHook[intnr].dbvmInterruptEmulation)
		{
			if (intnr==1)
				vmx_redirect_interrupt1(virt_differentInterrupt, 1, 0, 0);
			else if (intnr==3)
				vmx_redirect_interrupt3(virt_differentInterrupt, 3, 0, 0); 
			else
				vmx_redirect_interrupt14(virt_differentInterrupt, 14, 0, 0); 

			return TRUE; 
		}

	

		{
			INT_VECTOR newVector;
			
			
			
			newVector.wHighOffset=(WORD)((DWORD)(InterruptHook[intnr].originalEIP >> 16));
			newVector.wLowOffset=(WORD)InterruptHook[intnr].originalEIP;
			newVector.wSelector=(WORD)InterruptHook[intnr].originalCS;

#ifdef AMD64
			newVector.TopOffset=(InterruptHook[intnr].originalEIP >> 32);
			newVector.Reserved=0;
#endif

			{
				IDT idt;	
				GetIDT(&idt);

				
				newVector.bAccessFlags=idt.vector[intnr].bAccessFlags;

				disableInterrupts();
				idt.vector[intnr]=newVector;
				enableInterrupts();				
			}

			DbgPrint("Restored\n");
		}
	}

	return TRUE;
}

int inthook_HookInterrupt(unsigned char intnr, int newCS, ULONG_PTR newEIP, PJUMPBACK jumpback)
{
	IDT idt;	
	GetIDT(&idt);
	DbgPrint("inthook_HookInterrupt for cpu %d (vmxusable=%d)\n",cpunr(), vmxusable);
#ifdef AMD64
	DbgPrint("interrupt %d newCS=%x newEIP=%llx jumpbacklocation=%p\n",intnr, newCS, newEIP, jumpback);
#else
	DbgPrint("interrupt %d newCS=%x newEIP=%x jumpbacklocation=%p\n",intnr, newCS, newEIP, jumpback);
#endif

	DbgPrint("InterruptHook[%d].hooked=%d\n", intnr, InterruptHook[intnr].hooked);

	if (!InterruptHook[intnr].hooked)
	{
		
		InterruptHook[intnr].originalCS=idt.vector[intnr].wSelector;
		InterruptHook[intnr].originalEIP=idt.vector[intnr].wLowOffset+(idt.vector[intnr].wHighOffset << 16);
#ifdef AMD64
		InterruptHook[intnr].originalEIP|=(UINT64)((UINT64)idt.vector[intnr].TopOffset << 32);
#endif
	}

	if (jumpback)
	{
		jumpback->cs=InterruptHook[intnr].originalCS;
		jumpback->eip=InterruptHook[intnr].originalEIP;
	}

	DbgPrint("vmxusable=%d\n", vmxusable);

	if (vmxusable && ((intnr==1) || (intnr==3) || (intnr==14)) )
	{	
		DbgPrint("VMX Hook path\n");
		
		switch (intnr)
		{
			case 1:
				vmx_redirect_interrupt1(virt_emulateInterrupt, 0, newCS, newEIP);
				break;

			case 3:
				vmx_redirect_interrupt3(virt_emulateInterrupt, 0, newCS, newEIP);
				break;

			case 14:
				vmx_redirect_interrupt14(virt_emulateInterrupt, 0, newCS, newEIP);
				break;
		}

		InterruptHook[intnr].dbvmInterruptEmulation=1;
	}
	else
	{
		

		
		INT_VECTOR newVector;
#ifdef AMD64
		if (intnr<32)
		{
			DbgPrint("64-bit: DBVM is not loaded and a non dbvm hookable interrupt is being hooked that falls below 32\n");
			return FALSE;
		}
#endif


		DbgPrint("sizeof newVector=%d\n",sizeof(INT_VECTOR));
		
		
		newVector.wHighOffset=(WORD)((DWORD)(newEIP >> 16));
		newVector.wLowOffset=(WORD)newEIP;
		newVector.wSelector=(WORD)newCS;
		newVector.bUnused=0;
		newVector.bAccessFlags=idt.vector[intnr].bAccessFlags; 

#ifdef AMD64
		newVector.TopOffset=(newEIP >> 32);
		newVector.Reserved=0;
#endif

		disableInterrupts(); 
		idt.vector[intnr]=newVector;
		enableInterrupts();	

		InterruptHook[intnr].dbvmInterruptEmulation=0;

		DbgPrint("int %d will now go to %x:%p\n",intnr, newCS, newEIP);

	}


	InterruptHook[intnr].hooked=1;
	return TRUE;
}
