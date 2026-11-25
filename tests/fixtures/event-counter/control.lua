-- Test fixture: counts the script build events other mods raise, and keeps the counts in the save.
script.on_init(function() storage.counts = { built = 0, revived = 0 } end)
script.on_event(defines.events.script_raised_built, function() storage.counts.built = storage.counts.built + 1 end)
script.on_event(defines.events.script_raised_revive, function() storage.counts.revived = storage.counts.revived + 1 end)
remote.add_interface("event-counter", { counts = function() return storage.counts end })
