@file:JsModule("@radix-ui/react-avatar")
@file:JsNonModule

package radix.ui

import react.ComponentType

/*

val MyAvatar = FC {
    Avatar {
        className = ClassName("inline-block w-12 h-12 rounded-full overflow-hidden border")

        AvatarImage {
            src = "https://example.com/avatar.png"
            alt = "User"
            className = ClassName("object-cover w-full h-full")
        }

        AvatarFallback {
            className = ClassName("flex items-center justify-center w-full h-full bg-gray-200 text-gray-600")
            +"JD"
        }
    }
}

*/

// ------------------------------
// Avatar Root
external interface AvatarProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Root")
external val Avatar: ComponentType<AvatarProps>

// ------------------------------
// Avatar Image
external interface AvatarImageProps : DefaultProps {
    var src: String?
    var alt: String?
    var onLoadingStatusChange: ((String) -> Unit)? // "idle" | "loading" | "loaded" | "error"
    var asChild: Boolean?
}

@JsName("Image")
external val AvatarImage: ComponentType<AvatarImageProps>

// ------------------------------
// Avatar Fallback
external interface AvatarFallbackProps : DefaultProps {
    var delayMs: Int?
    var asChild: Boolean?
}

@JsName("Fallback")
external val AvatarFallback: ComponentType<AvatarFallbackProps>
