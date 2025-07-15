-- Minimal test for global table access
local x = _G["print"]
return x ~= nil
