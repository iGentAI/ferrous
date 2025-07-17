-- Simple table loop test with explicit step value
local t = {10, 20, 30, 40, 50}

-- Print table elements individually
print("Table elements accessed directly:")
print("t[1] =", t[1])
print("t[2] =", t[2])
print("t[3] =", t[3])
print("t[4] =", t[4])
print("t[5] =", t[5])

-- Print array length
print("Table length =", #t)

-- Print elements using a for loop with numeric indices and explicit step value
print("\nArray elements using numeric for loop with explicit step:")
for i = 1, #t, 1 do  -- Explicitly use step 1
  print(string.format("  t[%d] = %s", i, t[i]))
end

return t