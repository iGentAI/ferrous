-- Test dynamic key access on _G
print("Starting dynamic global access test...")

-- Direct access to function
local direct = print
print("Direct function access works")

-- Global table lookup with string literal
local global_lookup = _G["print"]
print("Global table lookup with string literal:", global_lookup ~= nil)

-- Dynamic string construction
local dynamic_name = "pr".."int"
print("Dynamic name:", dynamic_name)

-- Global lookup with dynamic string
local dynamic_lookup = _G[dynamic_name]
print("Global lookup with dynamic name:", dynamic_lookup ~= nil)

-- Test if they're all the same function
print("Functions are equal:", direct == global_lookup and direct == dynamic_lookup)

-- Test if functions work
if dynamic_lookup then
    dynamic_lookup("Called via dynamic lookup")
else
    print("ERROR: dynamic lookup failed")
end

return direct ~= nil and global_lookup ~= nil and dynamic_lookup ~= nil
