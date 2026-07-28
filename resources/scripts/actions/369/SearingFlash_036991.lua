-- 灼热之闪 / Searing Flash (Solar Bahamut)
POTENCY = 700

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_MAGIC, player.parameters:calc_magical_damage(POTENCY))

    return effects
end
