-- Bloodletter (BRD, ClassJob 23) - Level 12 ability
-- Potency: 130 (single target)
-- Recast: 15s (CooldownGroup 10), shared with Rain of Death
POTENCY = 130

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(POTENCY))

    return effects
end
