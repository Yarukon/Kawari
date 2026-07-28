-- Heavy Shot (ARC/BRD, ClassJob 5/23) - Level 1 weaponskill
-- Potency: 160 (before Burst Shot upgrade at level 76)
POTENCY = 160

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(POTENCY))

    return effects
end
