# Exception UI live target

This fixture raises a real `0xC0000005` access violation every three seconds. Its persistent Python
plugin observes the event and installs a real `exception.handle` interceptor
that returns `None`, leaving final disposition to the target's native SEH
handler. The v3 target also exports `RecoveryPoint`: a UI/Hub-owned interceptor
can redirect `rip/rsp` there to prove synchronous takeover while the original
SEH path remains available when no patch is returned. It keeps the Exception
monitor, route, callback result and script output populated while the UI is
inspected.
