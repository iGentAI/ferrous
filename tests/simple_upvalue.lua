-- Simplest possible upvalue test
local x = 42

local function get_x()
    return x
end

print("x =", x)
print("get_x() =", get_x())