-- Minimal tostring function test
local result = tostring(nil)
print("tostring(nil) = " .. result)

result = tostring(42)
print("tostring(42) = " .. result)

result = tostring(true)
print("tostring(true) = " .. result)

result = tostring("hello")
print("tostring(\"hello\") = " .. result)

return "Tostring function test successful"