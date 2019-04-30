target remote :2331
set remotetimeout 5
set print asm-demangle on
monitor semihosting enable
monitor semihosting IOClient 2
load
monitor reset