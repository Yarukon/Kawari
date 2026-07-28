-- 烈日核爆 / Sunflare (Solar Bahamut finisher)
POTENCY = 1000

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_MAGIC, player.parameters:calc_magical_damage(POTENCY))

    return effects
end
