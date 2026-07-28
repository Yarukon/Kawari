-- Sidewinder (BRD, ClassJob 23) - Level 60 ability
-- Potency: 400 (flat; DoT-conditional bonus was removed from retail years ago)
-- Recast: 60s (CooldownGroup 13)
POTENCY = 400

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(POTENCY))

    return effects
end
