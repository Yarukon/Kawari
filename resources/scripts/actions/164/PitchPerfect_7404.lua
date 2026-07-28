-- Pitch Perfect (BRD, ClassJob 23) - Level 52 ability
-- Potency by Repertoire stack: 1 -> 100, 2 -> 220, 3 -> 360.
-- Recast: 1s (CooldownGroup 1). Requires Wanderer's Minuet active and Repertoire proc.
-- Consumes all Repertoire stacks.
REPERTOIRE_STATUS = 3137
POTENCY_BY_STACK = {100, 220, 360}

function doAction(player, in_combo)
    effects = EffectsBuilder()
    local stacks = player:bard_repertoire()
    if stacks < 1 then stacks = 1 end
    if stacks > 3 then stacks = 3 end
    local potency = POTENCY_BY_STACK[stacks]
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(potency))
    effects:lose_effect(REPERTOIRE_STATUS, 0)

    return effects
end
