-- Minimal TFORLOOP test to validate the implementation fix

local t = {10, 20, 30}

-- Create our own basic iterator to avoid issues with pairs/ipairs
local function my_iter(t, i)
    if not i then i = 0 end
    i = i + 1
    local v = t[i]
    if v then
        return i, v
    end
    return nil
end

local function my_ipairs(t)
    -- Return the iterator triplet: function, state, initial control value
    return my_iter, t, nil
end

print("Starting minimal TFORLOOP test")

local sum = 0
for i, v in my_ipairs(t) do
    print("  iteration:", i, v)
    sum = sum + v
end

print("Sum:", sum)
return sum  -- Should be 60