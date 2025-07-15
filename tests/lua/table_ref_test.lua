-- Simple table reference test

print("Creating tables...")
local t1 = {}
t1.value = "test value"

local t2 = {}
t2.ref = t1

print("t1.value:", t1.value)
print("t2.ref:", t2.ref)
print("t2.ref.value:", t2.ref.value)

return t2.ref == t1
