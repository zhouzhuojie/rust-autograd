//! Provides helper functions for testing.
use crate::runtime::Feed;
use crate::tensor::Tensor;
use crate::{ndarray_ext, Float, Graph};

/// Checks the validity of `gradients` with finite difference trick.
/// For this test only, `variables` must be *shared* variables.
pub fn check_theoretical_grads<'g, 't, 'v, T: Float, A>(
    objective: A,
    gradients: &'t [A],
    variables: &'t [A],
    feeds: &'v [Feed<'v, T>],
    eps: T,
    tol: T,
    ctx: &'g Graph<T>,
) where
    A: AsRef<Tensor<'g, T>> + Copy,
{
    let objective = ctx.reduce_sum_to_scalar(objective);
    // backprop
    let theoretical_grads = ctx.eval(gradients, feeds);

    // for each variable nodes
    for (var_node, th_grad) in variables.iter().zip(theoretical_grads) {
        // Copy gradient array if needed
        let th_copied = if th_grad.as_ref().unwrap().is_standard_layout() {
            None
        } else {
            Some(ndarray_ext::deep_copy(&th_grad.as_ref().unwrap().view()))
        };
        let th_ptr = if let Some(ref inner) = th_copied {
            inner.as_ptr()
        } else {
            th_grad.as_ref().unwrap().as_ptr()
        };

        // for each values
        let v_len = ctx
            .env()
            .get_variable(
                var_node
                    .as_ref()
                    .get_variable_id()
                    .expect("This is not a variable"),
            )
            .borrow()
            .len();

        for i in 0..v_len as isize {
            let evacuated;
            // +
            unsafe {
                let mut guard_mut = ctx
                    .env()
                    .get_variable(
                        var_node
                            .as_ref()
                            .get_variable_id()
                            .expect("This is not a variable"),
                    )
                    .borrow_mut();
                let head = guard_mut.as_mut_ptr();
                evacuated = *head.offset(i);
                *head.offset(i) = evacuated + eps;
            }

            // eval
            let obj_pos_orig = objective.eval(feeds, ctx).unwrap();
            let obj_pos = if obj_pos_orig.is_standard_layout() {
                obj_pos_orig
            } else {
                ndarray_ext::deep_copy(&obj_pos_orig.view())
            };

            // -
            unsafe {
                let mut guard_mut = ctx
                    .env()
                    .get_variable(
                        var_node
                            .as_ref()
                            .get_variable_id()
                            .expect("This is not a variable"),
                    )
                    .borrow_mut();

                let head = guard_mut.as_mut_ptr();
                *head.offset(i) = evacuated - eps;
            }

            // eval
            let obj_neg_orig = objective.eval(feeds, ctx).unwrap();
            let obj_neg = if obj_neg_orig.is_standard_layout() {
                obj_neg_orig
            } else {
                ndarray_ext::deep_copy(&obj_neg_orig.view())
            };

            // restore
            unsafe {
                let mut guard_mut = ctx
                    .env()
                    .get_variable(
                        var_node
                            .as_ref()
                            .get_variable_id()
                            .expect("This is not a variable"),
                    )
                    .borrow_mut();
                let head = guard_mut.as_mut_ptr();
                *head.offset(i) = evacuated;
            }

            let two = T::one() + T::one();
            let g_num = (obj_pos - obj_neg).scalar_sum() / (two * eps);
            let g_th = unsafe { *th_ptr.offset(i) };

            // compare
            let diff = (g_num - g_th).abs();
            if diff > tol {
                panic!(
                    "Gradient checking failed with too large error: numerical={}, theoretical={}",
                    g_num, g_th
                );
            }
        }
    }
}
