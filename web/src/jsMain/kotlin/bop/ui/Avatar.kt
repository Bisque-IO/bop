package bop.ui

import lib.radix.AvatarFallbackProps
import lib.radix.AvatarImageProps
import lib.radix.AvatarPrimitiveImage
import lib.radix.AvatarPrimitiveRoot
import react.FC

val Avatar = FC<DefaultProps>("Avatar") { props ->
   AvatarPrimitiveRoot {
      spread(props, "className")
      dataSlot = "avatar"
      className = cn("relative flex size-8 shrink-0 overflow-hidden rounded-full", props.className)
   }
}

val AvatarImage = FC<AvatarImageProps>("AvatarImage") { props ->
   AvatarPrimitiveImage {
      spread(props, "className")
      dataSlot = "avatar-image"
      className = cn("aspect-square size-full", props.className)
   }
}

val AvatarFallback = FC<AvatarFallbackProps>("AvatarFallback") { props ->
   AvatarPrimitiveImage {
      spread(props, "className")
      dataSlot = "avatar-fallback"
      className = cn("bg-muted flex size-full items-center justify-center rounded-full", props.className)
   }
}
