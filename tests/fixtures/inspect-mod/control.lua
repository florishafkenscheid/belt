-- Test fixture: on the first tick of a --test-save run, print facts about the loaded save as
-- one JSON line, then end the run.

-- The save's characters, seen before the test run adds a player (and its character) at spawn.
local saved_characters = {}
script.on_init(function()
  for _, character in pairs(game.surfaces["nauvis"].find_entities_filtered({ type = "character" })) do
    saved_characters[#saved_characters + 1] = { x = character.position.x, y = character.position.y }
  end
end)

script.on_event(defines.events.on_tick, function()
  local surface = game.surfaces["nauvis"]
  local names = {}
  for _, entity in pairs(surface.find_entities_filtered({ force = "player" })) do
    names[entity.name] = (names[entity.name] or 0) + 1
  end
  local drills = {}
  for _, drill in pairs(surface.find_entities_filtered({ type = "mining-drill" })) do
    local under = surface.find_entities_filtered({ position = drill.position, type = "resource" })[1]
    local modules = {}
    local inventory = drill.get_module_inventory()
    for index = 1, inventory and #inventory or 0 do
      local stack = inventory[index]
      if stack.valid_for_read then modules[#modules + 1] = stack.name .. "@" .. stack.quality.name end
    end
    local radius = drill.prototype.mining_drill_radius
    local area = { { drill.position.x - radius, drill.position.y - radius }, { drill.position.x + radius, drill.position.y + radius } }
    local in_area = {}
    for _, resource in pairs(surface.find_entities_filtered({ area = area, type = "resource" })) do in_area[resource.name] = true end
    local area_resources = {}
    for name in pairs(in_area) do area_resources[#area_resources + 1] = name end
    table.sort(area_resources)
    drills[#drills + 1] = {
      name = drill.name, x = drill.position.x, y = drill.position.y,
      resource = under and under.name or "none",
      area_resources = table.concat(area_resources, ","),
      modules = table.concat(modules, ","),
    }
  end
  table.sort(drills, function(left, right) return left.x < right.x or (left.x == right.x and left.y < right.y) end)
  local characters = saved_characters
  local robots = {}
  for _, network in pairs(game.forces.player.logistic_networks["nauvis"] or {}) do
    robots[#robots + 1] = network.all_logistic_robots
  end
  local area = { left = math.huge, top = math.huge, right = -math.huge, bottom = -math.huge }
  for _, entity in pairs(surface.find_entities_filtered({ force = "player", type = { "beacon", "assembling-machine", "transport-belt", "mining-drill", "pipe" } })) do
    area.left = math.min(area.left, entity.position.x); area.top = math.min(area.top, entity.position.y)
    area.right = math.max(area.right, entity.position.x); area.bottom = math.max(area.bottom, entity.position.y)
  end
  print("BELT-INSPECT " .. helpers.table_to_json({
    tick = game.tick,
    surfaces = (function() local names = {} for name in pairs(game.surfaces) do names[#names + 1] = name end table.sort(names) return names end)(),
    speed = game.speed,
    entities = names,
    ghosts = surface.count_entities_filtered({ type = { "entity-ghost", "tile-ghost" } }),
    proxies = surface.count_entities_filtered({ type = "item-request-proxy" }),
    drills = drills,
    characters = characters,
    robots = robots,
    area = area.left ~= math.huge and area or nil,
    events = remote.interfaces["event-counter"] and remote.call("event-counter", "counts") or nil,
  }))
  script.finish_test(true)
end)
