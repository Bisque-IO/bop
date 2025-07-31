package lib.sonner

import js.objects.unsafeJso
import react.FC
import react.Props

external interface ToastMessage

fun ToastMessage(message: String): ToastMessage {
   return message.unsafeCast<ToastMessage>()
}

fun <P: Props> ToastMessage(message: FC<P>): ToastMessage {

   toast(ToastMessage(""), unsafeJso())
   return message.unsafeCast<ToastMessage>()
}
