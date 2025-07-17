-- Minimal test for verified standard library functions
print("===== Ferrous Lua Standard Library Test =====")

-- Test print function
print("Testing print function - this line shows it works!")

-- Test type function
print("\nTesting type function:")
print("type(nil) =", type(nil))
print("type(42) =", type(42))
print("type('hello') =", type("hello"))
print("type({}) =", type({}))
print("type(print) =", type(print))

-- Test tostring function
print("\nTesting tostring function:")
print("tostring(nil) =", tostring(nil))
print("tostring(42) =", tostring(42))
print("tostring(true) =", tostring(true))

-- Test basic function definition
print("\nTesting function definition:")
local function add(a, b)
  return a + b
end
print("add(2, 3) =", add(2, 3))

-- Basic table test (no iteration needed)
print("\nTesting basic tables:")
local t = {a = 1, b = 2}
print("t.a =", t.a)
print("t.b =", t.b)
t.c = 3
print("t.c =", t.c)

-- Test metatable
print("\nTesting metatable:")
local mt = {}
setmetatable(t, mt)
print("getmetatable(t) == mt:", getmetatable(t) == mt)

print("\n===== Test Completed Successfully =====")
return "Testing completed successfully"