{
  "general": { "model": "TURNING CENTER 2-AXIS", "vendor": "Camcraft", "kind": "lathe" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "bed", "type": "table", "parts": [
        { "id": "turret", "name": "X", "type": "linear", "travel": { "min": -2.5, "max": 320.0 }, "parts": [
          { "id": "car", "name": "Z", "type": "linear", "travel": { "min": -615.0, "max": 615.0 } }
        ] },
        { "id": "ms", "name": "head", "type": "head",
          "spindle": { "min_speed": 25.0, "max_speed": 5000.0, "power": 15.0,
                       "body_diameter": 0.0, "body_length": 0.0 },
          "tool_station": { "max_tool_length": 150.0, "max_tool_diameter": 0.0,
            "coolants": [ { "name": "Flood", "mCode": "M8" } ] } }
      ] }
    ] } },
  "controller": { "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 3000.0, "max_rapid_speed": 12000.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 4000.0, "max_rapid_speed": 18000.0 }
  } } },
  "tooling": { "number_of_tools": 12 }
}
