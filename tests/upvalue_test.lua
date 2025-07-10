-- Test 1: Simple upvalue capture
local x = 10

local function capture_x()
    return x  -- This should capture 'x' as an upvalue
end

print("Test 1: Capturing local variable")
print("x =", x)
print("capture_x() =", capture_x())

-- Test 2: Nested upvalue capture
local y = 20

local function outer()
    local z = 30
    
    local function inner()
        -- This should capture both 'y' (from global scope) and 'z' (from outer scope)
        return y + z
    end
    
    return inner
end

print("\nTest 2: Nested upvalue capture")
local inner_fn = outer()
print("y =", y)
print("inner_fn() =", inner_fn())  -- Should print 50

-- Test 3: Upvalue mutation
local counter = 0

local function increment()
    counter = counter + 1
    return counter
end

print("\nTest 3: Upvalue mutation")
print("Initial counter =", counter)
print("increment() =", increment())  -- Should print 1
print("increment() =", increment())  -- Should print 2
print("Final counter =", counter)    -- Should print 2

-- Test 4: Multiple closures sharing an upvalue
local shared = 100

local function make_adder(n)
    return function()
        shared = shared + n
        return shared
    end
end

local add10 = make_adder(10)
local add5 = make_adder(5)

print("\nTest 4: Shared upvalue")
print("Initial shared =", shared)
print("add10() =", add10())  -- Should print 110
print("add5() =", add5())     -- Should print 115
print("Final shared =", shared) -- Should print 115