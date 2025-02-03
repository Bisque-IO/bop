package bop.kernel;

import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;

@Retention(RetentionPolicy.RUNTIME)
public @interface Lifetime {

  Managed value() default Managed.MANUALLY;
}
