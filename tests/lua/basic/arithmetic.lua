-- Arithmetic Operations Test
-- Tests basic arithmetic operations

local a = 10
local b = 3

local sum = a + b
local diff = a - b
local product = a * b
local quotient = a / b
local mod = a % b
local power = a ^ 2

print("Sum:", sum)
print("Difference:", diff)
print("Product:", product)
print("Quotient:", quotient)
print("Modulo:", mod)
print("Power:", power)

return {
  sum = sum,
  diff = diff,
  product = product,
  quotient = quotient,
  mod = mod,
  power = power
}