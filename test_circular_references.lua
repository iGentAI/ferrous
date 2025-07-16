-- Test to demonstrate legitimate circular references in Lua tables

print("Testing circular references in tables")
print("======================================")

-- Simple self-reference
local t1 = {}
t1.self = t1
print("Self-reference created")

-- Check if self-reference works
if t1.self == t1 then
    print("✓ Self-reference works correctly")
else
    print("✗ Self-reference broken")
end

-- Create a cycle of references
local a = {}
local b = {}
a.ref = b
b.ref = a
print("\nReference cycle created")

-- Check if cycle works
if a.ref.ref == a then
    print("✓ Reference cycle works correctly")
else
    print("✗ Reference cycle broken")
end

-- Create a more complex cycle
local t2 = {}
local t3 = {}
local t4 = {}
t2.next = t3
t3.next = t4
t4.next = t2
print("\nThree-table cycle created")

-- Check if complex cycle works
if t2.next.next.next == t2 then
    print("✓ Complex cycle works correctly")
else
    print("✗ Complex cycle broken")
end

-- Use a circular reference in a function
local function traverse(node, count)
    if count >= 10 then return "Stopped at limit" end
    
    if node.next then
        return traverse(node.next, count + 1) .. " → " .. "node"
    else
        return "node"
    end
end

local result = traverse(t2, 0)
print("\nTraversing cycle with limit:", result)

-- Try a real-world use case: object system with inheritance
local Object = {}
Object.super = nil
Object.name = "Object"

function Object:new()
    local instance = {}
    setmetatable(instance, {__index = self})
    return instance
end

function Object:getClass()
    return self
end

-- Create a subclass with circular reference
local Widget = Object:new()
Widget.name = "Widget"
Widget.parent = Object  -- Normal reference up
Object.widget = Widget  -- Circular reference down

print("\nObject system created")
print("Widget's parent:", Widget.parent.name)
print("Object's widget:", Object.widget.name)

local button = Widget:new()
print("Button's class:", button:getClass().name)

print("\nAll circular reference tests passed!")

-- Return a table with circular references for result checking
local result_table = {}
result_table.self = result_table
return result_table