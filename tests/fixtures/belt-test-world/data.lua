-- Stand-ins for Editor Extensions' test entities: one it shows, one it hides.
local shown = table.deepcopy(data.raw["infinity-container"]["infinity-chest"])
shown.name = "ee-infinity-chest"
shown.hidden = false
local hidden = table.deepcopy(data.raw["infinity-container"]["infinity-chest"])
hidden.name = "ee-infinity-helper"
hidden.hidden = true
data:extend({ shown, hidden })
