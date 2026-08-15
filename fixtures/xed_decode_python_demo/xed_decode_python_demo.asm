PUBLIC DecodeTarget

.code

DecodeTarget PROC
    lea rax, DecodeTarget
    db 00fh, 01ch, 000h ; CLDEMOTE [rax] when the pre-decode input is enabled
    ret
DecodeTarget ENDP

END
