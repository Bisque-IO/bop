import bop.ui.InputOTP
import bop.ui.InputOTPGroup
import bop.ui.InputOTPSeparator
import bop.ui.InputOTPSlot
import bop.ui.Label
import inputotp.REGEXP_ONLY_DIGITS
import react.FC
import react.dom.html.ReactHTML.div
import web.cssom.ClassName

val InputOTPDemo = FC {
   div {
      className = ClassName("pt-6 flex flex-col flex-wrap gap-6 md:flex-row")

      InputOTPSimple {}
      InputOTPPattern {}
      InputOTPWithSpacing {}
   }
}

val InputOTPSimple = FC {
   div {
      className = ClassName("grid gap-2")

      Label {
         htmlFor = "simple"
         +"Simple"
      }

      InputOTP {
         id = "simple"
         maxLength = 6

         InputOTPGroup {
            InputOTPSlot { index = 0; hasFakeCaret = true }
            InputOTPSlot { index = 1 }
            InputOTPSlot { index = 2 }
         }
         InputOTPSeparator {}
         InputOTPGroup {
            InputOTPSlot { index = 3 }
            InputOTPSlot { index = 4 }
            InputOTPSlot { index = 5 }
         }
      }
   }
}

val InputOTPPattern = FC {
   div {
      className = ClassName("grid gap-2")

      Label {
         htmlFor = "digits-only"
         +"Digits Only"
      }

      InputOTP {
         id = "digits-only"
         maxLength = 6
         pattern = REGEXP_ONLY_DIGITS

         InputOTPGroup {
            InputOTPSlot { index = 0 }
            InputOTPSlot { index = 1 }
            InputOTPSlot { index = 2 }
            InputOTPSlot { index = 3 }
            InputOTPSlot { index = 4 }
            InputOTPSlot { index = 5 }
         }
      }
   }
}

val InputOTPWithSpacing = FC {
   div {
      className = ClassName("grid gap-2")

      Label {
         htmlFor = "with-spacing"
         +"With Spacing"
      }

      InputOTP {
         id = "with-spacing"
         maxLength = 6

         InputOTPGroup {
            className = ClassName("gap-2 *:data-[slot=input-otp-slot]:rounded-md *:data-[slot=input-otp-slot]:border")
            InputOTPSlot {
               index = 0
               ariaInvalid = true
            }
            InputOTPSlot {
               index = 1
               ariaInvalid = true
            }
            InputOTPSlot {
               index = 2
               ariaInvalid = true
            }
            InputOTPSlot {
               index = 3
               ariaInvalid = true
            }
         }
      }
   }
}