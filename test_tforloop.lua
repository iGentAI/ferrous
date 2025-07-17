-- Simple TFORLOOP test with ipairs
local arr = {10, 20, 30}
local sum = 0

-- This uses TFORLOOP opcode
for i, v in ipairs(arr) do
    print(string.format("Index: %d, Value: %d", i, v))
    sum = sum + v
end

print("Sum:", sum)
return {success = true, sum = sum}