-- Test basic arithmetic and control flow

-- Arithmetic
local a = 10
local b = 20
local add = a + b
local sub = a - b
local mul = a * b
local div = a / b

print("Arithmetic test:")
print("  Add:", add)
print("  Sub:", sub)
print("  Mul:", mul)
print("  Div:", div)

-- Control flow
local max
if a > b then
  max = a
else
  max = b
end

print("Control flow test:")
print("  Max of", a, "and", b, "is", max)

-- Loop
local sum = 0
for i = 1, 5 do
  sum = sum + i
end

print("Loop test:")
print("  Sum 1 to 5:", sum)

return add + sub + mul + div + max + sum
