-- Debug the ipairs functionality and register handling
local t = {10, 20, 30}
print("Table:", t)        -- Print the table handle

-- Debug registers before ipairs
local saved_ipairs = ipairs
local result = 0

-- Loop and sum
for i, v in saved_ipairs(t) do
    result = result + v
end

print("Result:", result)
return result