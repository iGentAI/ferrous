-- Minimal _G access test

print("Testing globals table access...")

-- Access print via _G
local p = _G.print

-- Use the retrieved function
if p then
  p("SUCCESS: _G.print works!")
  return true
else
  print("FAILURE: _G.print is nil!")
  return false
end
