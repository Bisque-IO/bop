@file:JsModule("@radix-ui/react-slider")
@file:JsNonModule

package radix.ui

import react.*

/*

val MySlider = FC {
    val (value, setValue) = useState(arrayOf(50.0))

    Slider {
        this.value = value
        onValueChange = setValue
        min = 0.0
        max = 100.0
        step = 1.0

        SliderTrack {
            SliderRange()
        }

        SliderThumb()
    }
}

*/

// ------------------------------
// Root
// ------------------------------
external interface SliderProps : DefaultProps {
    var value: Array<Double>?
    var defaultValue: Array<Double>?
    var onValueChange: ((Array<Double>) -> Unit)?
    var onValueCommit: ((Array<Double>) -> Unit)?
    var min: Double?
    var max: Double?
    var step: Double?
    var orientation: String? // "horizontal" | "vertical"
    var disabled: Boolean?
    var inverted: Boolean?
    var name: String?
    var minStepsBetweenThumbs: Int?
    var dir: String? // "ltr" | "rtl"
    var asChild: Boolean?
}

@JsName("Root")
external val Slider: ComponentType<SliderProps>

// ------------------------------
// Track
// ------------------------------
external interface SliderTrackProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Track")
external val SliderTrack: ComponentType<SliderTrackProps>

// ------------------------------
// Range (active portion of track)
// ------------------------------
external interface SliderRangeProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Range")
external val SliderRange: ComponentType<SliderRangeProps>

// ------------------------------
// Thumb (handle)
// ------------------------------
external interface SliderThumbProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Thumb")
external val SliderThumb: ComponentType<SliderThumbProps>
