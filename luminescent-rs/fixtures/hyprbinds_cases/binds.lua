local menu = "fuzzel"
local function ws(n) return mainMod .. " + " .. n end

hl.bind(mainMod .. " + Q", hl.dsp.exec_cmd(TERMINAL))
hl.bind(mainMod .. " + G", hl.dsp.exec_cmd("uwsm app -- " .. BROWSER))
hl.bind(mainMod .. " + C", hl.dsp.window.close())
hl.bind("SUPER + SHIFT + E", hl.dsp.exit())
hl.bind(mainMod .. " + F", hl.dsp.window.fullscreen({ mode = 1 }))
hl.bind(mainMod .. " + left", hl.dsp.focus({ direction = "left" }))
hl.bind("ALT + R", hl.dsp.exec_cmd(menu .. " --drun"))  -- a comment
hl.bind("CTRL + ALT + T", function() hl.dsp.exec_cmd("notify-send hi") end)
--[[ hl.bind("SUPER + X", hl.dsp.exit()) ]]
-- hl.bind("SUPER + Y", hl.dsp.exit())
for i = 1, 3 do
    local key = i
    hl.bind(mainMod .. " + " .. key, hl.dsp.focus({ workspace = i }))
end
hl.bind(mainMod .. " + mouse:272", hl.dsp.window.drag(), { mouse = true })
hl.bind("SUPER + code:10", hl.dsp.focus({ workspace = 1 })); hl.bind("SUPER + D", hl.dsp.exec_cmd(FILES))
hl.bind("SUPER + V", hl.dsp.exec_cmd("cliphist list | " .. menu .. " --dmenu | cliphist decode | wl-copy"))
