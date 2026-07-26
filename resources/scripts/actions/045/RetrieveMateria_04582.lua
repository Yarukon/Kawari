function doAction(player, in_combo)
    -- The client already told us which item to retrieve from with ClientTrigger 2800.
    -- The actual retrieval happens in the handler's on_yield when the client sends EventAction1
    -- after playing the retrieval animation.
    return EffectsBuilder()
end
