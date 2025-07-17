-- Test script for the fixed for loop with table array access

-- Create a table to iterate
local t = {10, 20, 30, 40, 50}
print("Table created with these elements:")
print("t[1] =", t[1])
print("t[2] =", t[2])
print("t[3] =", t[3])
print("t[4] =", t[4]) 
print("t[5] =", t[5])
print("Table length (#t) =", #t)

print("\n--- Test 1: Basic for loop without explicit step (using default) ---")
local sum = 0
for i = 1, #t do  -- Note: No explicit step here, should use default 1
  print(string.format("Iteration %d: t[%d] = %s", i, i, t[i]))
  sum = sum + t[i]
end
print("Sum of all elements:", sum)
assert(sum == 150, "Expected sum to be 150")

print("\n--- Test 2: For loop with explicit step ---")
local oddSum = 0
for i = 1, #t, 2 do  -- Only odd indices: 1, 3, 5
  print(string.format("Iteration %d: t[%d] = %s", i, i, t[i]))
  oddSum = oddSum + t[i]
end
print("Sum of elements at odd indices:", oddSum)
assert(oddSum == 90, "Expected odd sum to be 90")  -- 10 + 30 + 50 = 90

print("\nAll for loop tests passed!")
return {success = true}