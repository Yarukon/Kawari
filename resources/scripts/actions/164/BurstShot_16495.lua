-- Burst Shot (BRD, ClassJob 23) - Level 76 weaponskill
-- Potency: 220 (upgrades from Heavy Shot at level 76)
POTENCY = 220

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(POTENCY))

    return effects
end
