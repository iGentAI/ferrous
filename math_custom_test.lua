-- Test to isolate the math.custom:double method call issue

-- Create math.custom table
math.custom = {}

-- Add a method to math.custom
function math.custom:double(x)
  -- Just a simple implementation that doubles the input
  return x * 2
end

-- Test the method call
local result = math.custom:double(7)
print("Result:", result)

-- Return success
return result == 14
