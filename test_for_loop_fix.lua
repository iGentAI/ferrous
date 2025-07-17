-- Ultra-focused test for for loop register coordination issue
local t = {10, 20, 30, 40, 50}
print("Initial table:", t[1], t[2], t[3])

-- Test 1: Simple for loop without complex initialization
print("\nTest 1: Simple for loop")
local sum1 = 0
for i = 1, 3 do
  sum1 = sum1 + i
  print("  Iteration", i, "sum =", sum1)
end
print("Final sum:", sum1)

-- Test 2: For loop with table access in body
print("\nTest 2: For loop with table access in body")
local sum2 = 0
for i = 1, 3 do
  sum2 = sum2 + t[i]
  print("  t[" .. i .. "] =", t[i], "sum =", sum2)
end
print("Final sum:", sum2)

return {success = true, sum1 = sum1, sum2 = sum2}
