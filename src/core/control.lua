
local util = require("util")
local bp = require("bp")

local save_complete = false
local bp_loaded = false
local start_tick = 0

local draw_bp = function()
    game.print('starting placement');
    local first_player = game.get_player(1);
    local s = first_player.surface;
    local f = first_player.force.name;
    local afterSpawns = {}
    
    local bp_entity = s.create_entity { name = 'item-on-ground', position = { 0, 0 }, stack = 'blueprint' }
    bp_entity.stack.import_stack(bp.bp_string)

    local first_ent = bp_entity.stack.get_blueprint_entities()[1]

    local bp_ghost = bp_entity.stack.build_blueprint { surface = s, force = f, position = first_player.position, force_build = true }
    local first_ghost = bp_ghost[1]
    local offset = {first_ghost.position.x - first_ent.position.x, first_ghost.position.y - first_ent.position.y}
    for _, bpe in pairs(bp_entity.stack.get_blueprint_entities()) do
      if bpe.name == 'big-mining-drill' and bpe.items ~= nil then
        for _, req in pairs(bpe.items) do
          if req.id.name == 'efficiency-module-3' and req.id.quality == nil then
            s.create_entity{name="stone", amount=10000000, position={bpe.position.x + offset[1], bpe.position.y + offset[2]}}
          elseif req.id.name == 'efficiency-module-3' and req.id.quality == 'uncommon' then
            s.create_entity{name="iron-ore", amount=10000000, position={bpe.position.x + offset[1], bpe.position.y + offset[2]}}
          elseif req.id.name == 'efficiency-module-3' and req.id.quality == 'rare' then
            s.create_entity{name="copper-ore", amount=10000000, position={bpe.position.x + offset[1], bpe.position.y + offset[2]}}
          elseif req.id.name == 'efficiency-module-3' and req.id.quality == 'epic' then
            s.create_entity{name="coal", amount=10000000, position={bpe.position.x + offset[1], bpe.position.y + offset[2]}}
          end
        end
      end
    end
    for _, entity in pairs(bp_ghost) do
		
        if (entity.ghost_name == 'locomotive' or entity.ghost_name == 'cargo-wagon' or entity.ghost_name == 'fluid-wagon') then
            table.insert(afterSpawns, entity)
        else
            if (entity ~= nil and entity.name == 'entity-ghost' and entity.ghost_type ~= nil and entity.item_requests ~= nil) then
                local items = util.table.deepcopy(entity.item_requests)

                local p, ri = entity.revive();
                if (ri ~= nil) then
                    for _, v in pairs(items) do
                        ri.get_module_inventory().insert({ name = v.name, count = v.count, quality = v.quality})
                    end
                end
            else
                entity.revive();
            end
        end

    end

    for _, entity in pairs(afterSpawns) do
        local r, to = entity.revive();
    end

    -- and now we need to rebuild stuff, so that we build the miners now that we have the proper ore under them
    afterSpawns = {}
    local bp_ghost2 = bp_entity.stack.build_blueprint { surface = s, force = f, position = first_player.position, force_build = true }
    bp_entity.destroy()

    for _, entity in pairs(bp_ghost2) do
        if (entity.ghost_name == 'locomotive' or entity.ghost_name == 'cargo-wagon' or entity.ghost_name == 'fluid-wagon') then
            table.insert(afterSpawns, entity)
        else
            if (entity ~= nil and entity.name == 'entity-ghost' and entity.ghost_type ~= nil and entity.item_requests ~= nil) then
                local items = util.table.deepcopy(entity.item_requests)

                local p, ri = entity.revive();
                if (ri ~= nil) then
                    for _, v in pairs(items) do
                        ri.get_module_inventory().insert({ name = v.name, count = v.count, quality = v.quality})
                    end
                end
            else
                entity.revive();
            end
        end
    end
    for _, entity in pairs(afterSpawns) do
        local r, to = entity.revive();
    end
	  game.print('placement done')
    
    -- Add logistic bots to each roboport
    if (bp.bots ~= nil and bp.bots > 0) then
        for _, roboport in pairs(s.find_entities_filtered({ type = "roboport" })) do
            roboport.insert({ name = "logistic-robot", count = bp.bots, quality = "legendary" })
        end
    end

end

script.on_event(defines.events.on_tick, function(event)
  if not game.is_multiplayer() then
    return
  end
  if not bp_loaded then
    start_tick = event.tick
    draw_bp()
    bp_loaded = true
  end
  if start_tick > 0 and not save_complete and bp.save_after_ticks > 0 and event.tick - start_tick >= bp.save_after_ticks then
    game.print('saving game');
    game.server_save(bp.save_game_name)
    save_complete = true
  end
end)