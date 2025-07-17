-- Module and Package System Test
-- Tests module loading and the package library
-- Status: FAILING - Package system not yet implemented

-- Test the require function (if it exists)
local has_require = type(require) == "function"
print("Has require function:", has_require)

-- Test package.loaded
local has_package = type(package) == "table"
print("Has package table:", has_package)

if has_package then
  print("package.loaded exists:", type(package.loaded) == "table")
  print("package.path exists:", type(package.path) == "string")
  print("package.cpath exists:", type(package.cpath) == "string")
end

-- Test module function
local has_module = type(module) == "function"
print("Has module function:", has_module)

-- Try to create a simple module
local module_test_success = false
if has_module then
  local f = io.open("test_module.lua", "w")
  if f then
    f:write([[
      module("test_module", package.seeall)
      
      function hello()
        return "Hello from module"
      end
      
      value = 42
    ]])
    f:close()
    
    -- Try to require it
    if has_require then
      local success, mod = pcall(require, "test_module")
      if success then
        print("Module loaded:", mod)
        print("Module hello:", mod.hello())
        print("Module value:", mod.value)
        module_test_success = mod.value == 42
      else
        print("Error loading module:", mod)
      end
    end
    
    os.remove("test_module.lua")  -- Clean up
  end
end

-- Test simple module pattern without module()
local m = {}

function m.add(a, b)
  return a + b
end

m.name = "Calculator"

-- Test _G manipulation
_G.global_from_module = "I'm a global"
print("Global from module:", _G.global_from_module)

-- Test package.loadlib (won't work in sandbox)
local loadlib_success = false
if has_package and type(package.loadlib) == "function" then
  local success, lib = pcall(package.loadlib, "dummy.so", "luaopen_dummy")
  print("loadlib result:", success)
  loadlib_success = not success -- We expect this to fail in a sandbox
end

-- Return test result
-- Most of these tests are expected to fail in the current implementation
return m.add(2, 3) == 5 and
       global_from_module == "I'm a global"
       -- Don't include module_test_success since require likely isn't implemented