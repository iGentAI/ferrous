-- Very simple method test
local t = {}

-- Add a value field and a triple method
t.value = 14
t.triple = function(self)
  return self.value * 3
end

-- Test the method call syntax
local result = t:triple()
print("Method call result:", result)

return result == 42
