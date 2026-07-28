-- 灵极脉冲 / Umbral Impulse GCD (Solar Bahamut filler)
POTENCY = 640

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_MAGIC, player.parameters:calc_magical_damage(POTENCY))

    return effects
end
