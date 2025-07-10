-- Minimal upvalue test that only uses an upvalue with no extra code

local x = 42

local function get()
    return x  -- x should be captured as an upvalue
end

return get()