-- STAGE: data
-- EXPECT_ERROR: sandbox
-- `sandbox(true)` freezes the standard libraries too.
string.format = function() return "hijacked" end
