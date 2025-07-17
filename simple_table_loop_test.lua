-- Simple table loop test
local t = {10, 20, 30, 40, 50}

-- Print elements using a for loop with numeric indices
print("Array elements using numeric for loop:")
for i = 1, #t do
  print(string.format("  t[%d] = %s", i, t[i]))
end

-- Return success
return {success = true}