-- Test recursive function traversal with string concatenation
print("Testing recursive traversal with string concatenation")

-- Create a simple cyclic structure
local t1 = {}
local t2 = {}
local t3 = {}
t1.next = t2
t2.next = t3
t3.next = t1

-- Simple verification that the cycle works
if t1.next.next.next == t1 then
    print("✓ Cycle created correctly")
else
    print("✗ Cycle broken")
end

-- Recursive function with string concatenation
local function traverse(node, count)
    print("Traverse depth:", count)  -- Print depth to track recursion
    
    -- Make sure we don't recurse too deeply
    if count >= 5 then 
        print("Reached recursion limit")
        return "LIMIT" 
    end
    
    -- If node has a .next property, recurse and concatenate
    if node.next then
        local result = traverse(node.next, count + 1)
        print("Building result at depth:", count)
        -- String concatenation is the suspected issue
        return result .. " -> Node" .. count
    else
        return "End"
    end
end

print("\nStarting traversal with explicit recursion limit...")
local result = traverse(t1, 0)
print("Traversal result:", result)

print("\nTest completed successfully")
return true