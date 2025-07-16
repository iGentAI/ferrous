-- Test circular references versus infinite recursion handling

print("PART 1: Legitimate Circular References (Should Work)")
print("===================================================")

-- Test 1: Self-reference
local t1 = {}
t1.self = t1
print("Test 1: Self-reference - " .. (t1.self == t1 and "PASSED" or "FAILED"))

-- Test 2: Circular reference chain
local t2, t3, t4 = {}, {}, {}
t2.next = t3
t3.next = t4
t4.next = t2  -- Creates a cycle
print("Test 2: Reference chain - " .. (t2.next.next.next == t2 and "PASSED" or "FAILED"))

-- Test 3: Traversal with cycle detection
function traverse_safely(node, visited, depth)
    visited = visited or {}
    depth = depth or 0
    
    -- Stop if we detect a cycle (this is how cycles should be handled)
    if visited[node] then
        return "<cycle>"
    end
    
    -- Record that we've visited this node
    visited[node] = true
    
    -- Recursively traverse if there's a next node
    if node.next then
        return "Node(" .. depth .. ") → " .. traverse_safely(node.next, visited, depth + 1)
    else
        return "End"
    end
end

print("\nTest 3: Safe traversal of circular structure:")
local result = traverse_safely(t2, {}, 0)
print(result)

print("\nPART 2: Properly bounded resource usage (Should Work)")
print("===================================================")

-- Test 4: Deep recursion that is still legitimate
function bounded_recursion(n)
    if n <= 0 then
        return "bottom"
    end
    return bounded_recursion(n - 1) .. " level " .. n
end

print("\nTest 4: Deep but bounded recursion:")
-- This is deep but still reasonable and should work
local bounded_result = bounded_recursion(10)
print("Result (length: " .. #bounded_result .. "): " .. bounded_result)

-- Test 5: Large but bounded string concatenation
print("\nTest 5: Large but bounded string concatenation:")
local s = ""
for i = 1, 20 do
    s = s .. i .. ", "
end
print("Result (length: " .. #s .. "): " .. s)

print("\nPART 3: Potentially infinite recursion (May Fail)")
print("===================================================")
print("NOTE: The following tests are expected to be caught by the VM")
print("      with resource limit errors, not infinite loops.")

-- Test 6: Attempt infinite recursion
print("\nTest 6: Attempting unbounded recursion...")
print("This should be caught by resource limits, not run forever.")

local function infinite_recursion(x)
    return infinite_recursion(x) -- No base case!
end

local ok, err = pcall(function()
    infinite_recursion(1)
    return "ERROR - Should not get here!"
end)

if not ok then
    print("Correctly caught infinite recursion: " .. err)
else
    print("Failed to catch infinite recursion!")
end

-- Test 7: Recursion with string concatenation
print("\nTest 7: Attempting string building without proper bounds...")

local function build_infinite_string(s)
    if #s > 1000000 then return s end -- Large but technically bounded
    return build_infinite_string(s .. s) -- Exponential growth
end

local ok, err = pcall(function()
    return build_infinite_string("x")
end)

if not ok then
    print("Correctly caught excessive string building: " .. err)
else
    print("Failed to catch excessive string building!")
end

print("\nPART 4: Safely traversing circular structures (Should Work)")
print("===================================================")

-- Test 8: Object system with circular references
local Object = {
    instances = {}
}

Object.__index = Object -- self reference in metatable

function Object:new()
    local instance = {}
    setmetatable(instance, self)
    table.insert(self.instances, instance) -- Instances reference their parent
    instance.parent = self -- Circular reference
    return instance
end

function Object:count_instances()
    return #self.instances
end

-- Create objects
local Widget = Object:new()
local Button = Widget:new()
local Checkbox = Widget:new()

-- Each object has potentially circular references, but legitimate
print("\nTest 8: Object system with circular references:")
print("Widget instance count: " .. Widget:count_instances())
print("Widget's parent: " .. (Widget.parent == Object and "Object" or "Unknown"))
print("Button's parent's parent: " .. (Button.parent.parent == Object and "Object" or "Unknown"))

print("\nAll tests completed!")