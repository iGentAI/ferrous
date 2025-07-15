-- Test dynamic string construction with globals lookup
local dynamic_name = "pr".."int"
print("Dynamic name constructed:", dynamic_name)

-- Try to access global with dynamic string
local dynamic_lookup = _G[dynamic_name]

if dynamic_lookup ~= nil then
    print("SUCCESS: Dynamic lookup works!")
    dynamic_lookup("This proves dynamic lookup works")
    return true
else
    print("FAIL: Dynamic lookup returned nil")
    return false
end
