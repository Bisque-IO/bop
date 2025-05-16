package bop.ui

import radix.ui.AvatarFallbackProps
import radix.ui.AvatarImageProps
import react.FC

val Avatar = FC<DefaultProps>("Avatar") { props ->
   radix.ui.Avatar {
      this["data-slot"] = "avatar"
      className = cn("relative flex size-8 shrink-0 overflow-hidden rounded-full", props.className)
      spread(props, "className")
   }
}

val AvatarImage = FC<AvatarImageProps>("AvatarImage") { props ->
   radix.ui.AvatarImage {
      this["data-slot"] = "avatar-image"
      className = cn("aspect-square size-full", props.className)
      spread(props, "className")
   }
}

val AvatarFallback = FC<AvatarFallbackProps>("AvatarFallback") { props ->
   radix.ui.AvatarImage {
      this["data-slot"] = "avatar-fallback"
      className = cn("bg-muted flex size-full items-center justify-center rounded-full", props.className)
      spread(props, "className")
   }
}
