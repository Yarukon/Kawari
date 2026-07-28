-- Radiant Encore (BRD, ClassJob 23) - Level 100 ability
-- Potency by Coda consumed: 1 -> 700, 2 -> 800, 3 -> 1100 (patch 7.51 values).
-- Requires: Radiant Encore Ready status (3863). Recast: 1s (CooldownGroup 1).
RADIANT_ENCORE_READY_STATUS = 3863
POTENCY_BY_CODA = {700, 800, 1100}

function doAction(player, in_combo)
    effects = EffectsBuilder()
    local coda = player:bard_radiant_encore_coda()
    if coda < 1 then coda = 1 end
    if coda > 3 then coda = 3 end
    local potency = POTENCY_BY_CODA[coda]
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(potency))
    effects:lose_effect(RADIANT_ENCORE_READY_STATUS, 0)

    return effects
end
