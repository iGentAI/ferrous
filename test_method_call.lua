-- Test method call syntax
print("Testing method call syntax")

-- Create a table with a method
local t = {}
t.value = 42

-- Define a method directly
t.double = function(self, x)
  return self.value * x
end

-- Test normal function call syntax
print("Normal call:", t.double(t, 2))

-- Test method call syntax
print("Method call:", t:double(2))

-- Return success if both calls work properly
return t.double(t, 2) == 84 and t:double(2) == 84
