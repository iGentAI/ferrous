-- Ultra minimal test for for loop with table access
local t = {10, 20, 30}

-- Single loop iteration with fixed table index
for i = 1, 1 do
  local v = t[1]  -- Direct access with constant index
  print("Table value at index 1:", v)
end

return "Success"