-- Minimal type function test
local result = type(nil)
print("type(nil) = " .. result)

result = type(42)
print("type(42) = " .. result)

result = type("hello")
print("type(\"hello\") = " .. result) 

result = type({})
print("type({}) = " .. result)

result = type(print)
print("type(print) = " .. result)

return "Type function test successful"