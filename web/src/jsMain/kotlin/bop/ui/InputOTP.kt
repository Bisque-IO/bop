package bop.ui

import inputotp.OTPInputProps
import js.objects.unsafeJso
import radix.ui.MinusIcon
import react.FC
import react.Props
import react.dom.aria.AriaRole
import react.dom.html.ReactHTML.div
import web.cssom.ClassName

val InputOTP = FC<OTPInputProps>("InputOTP") { props ->
   inputotp.OTPInput {
      dataSlot = "input-otp"
      containerClassName = cn("flex items-center gap-2 has-disabled:opacity-50", props.containerClassName)
      className = cn("disabled:cursor-not-allowed", props.className)
      spread(props, "className", "containerClassName")
   }
}

val InputOTPGroup = FC<DefaultProps>("InputOTPGroup") { props ->
   div {
      dataSlot = "input-otp-group"
      className = cn("flex items-center", props.className)
      spread(props, "className")
   }
}

external interface InputOTPSlotProps : inputotp.SlotProps {
   var index: Int?
   @JsName("aria-invalid")
   var ariaInvalid: Boolean?
}

private val EXCLUDE = setOf("className", "children", "hasFakeCaret", "isActive")
val InputOTPSlot = FC<InputOTPSlotProps>("InputOTPSlot") { props ->
   val inputOTPContext = react.use(inputotp.OTPInputContext)
   if (props.index == null) throw IllegalArgumentException("InputOTPSlotProps.index must be non-null.")
   val slot = inputOTPContext.slots[props.index!!]

   div {
      spread(props, EXCLUDE)
      dataSlot = "input-otp-slot"
      dataActive = slot.isActive
      className = cn(
         "data-[active=true]:border-ring data-[active=true]:ring-ring/50 data-[active=true]:aria-invalid:ring-destructive/20 dark:data-[active=true]:aria-invalid:ring-destructive/40 aria-invalid:border-destructive data-[active=true]:aria-invalid:border-destructive dark:bg-input/30 border-input relative flex h-9 w-9 items-center justify-center border-y border-r text-sm shadow-xs transition-all outline-none first:rounded-l-md first:border-l last:rounded-r-md data-[active=true]:z-10 data-[active=true]:ring-[3px]",
         props.className
      )

      +slot.char
      if (slot.hasFakeCaret) {
         div {
            className = ClassName("pointer-events-none absolute inset-0 flex items-center justify-center")

            div {
               className = ClassName("animate-caret-blink bg-foreground h-6 w-px pr-1 duration-1000")
               +"|"
            }
         }
      }
   }
}

val InputOTPSeparator = FC<Props>("InputOTPSeparator") { props ->
   div {
      dataSlot = "input-otp-separator"
      role = AriaRole.separator
      spread(props, "role")
      MinusIcon {}
   }
}