-- Minimal test for method call
local t = {}

-- Add a value field and a double method
t.value = 42
t.double = function(self, x)
  -- Debug to see what self is
  print("Inside double method, self.value = " .. self.value)
  print("Parameter x = " .. x)
  return self.value * x
end

-- Test the method call syntax
local result = t:double(3)
print("Method call result:", result)

return result == 126
