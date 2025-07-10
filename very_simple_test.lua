-- Absolute minimum test - just manual array access
local t = {10, 20, 30}
local sum = 0

-- Just access by indices
sum = t[1] + t[2] + t[3]

return sum  -- Should be 60