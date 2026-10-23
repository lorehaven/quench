//! Arithmetic in GF(256) with the QR polynomial x^8 + x^4 + x^3 + x^2 + 1 (0x11D), and the Reed-Solomon
//! remainder a block's error-correction codewords are made of.

/// `a * b` in the field (shift-and-add, reducing as it goes).
const fn multiply(a: u8, b: u8) -> u8 {
    let mut product = 0u8;
    let mut i = 8;
    while i > 0 {
        i -= 1;
        // product = product * 2 (reduced), then add a if bit i of b is set.
        let carry = product & 0x80 != 0;
        product <<= 1;
        if carry {
            product ^= 0x1d;
        }
        if (b >> i) & 1 == 1 {
            product ^= a;
        }
    }
    product
}

/// The generator polynomial of degree `degree` (without its leading 1): the coefficients of
/// (x - 1)(x - a)(x - a^2)...(x - a^(degree-1)), highest power first.
fn generator(degree: usize) -> Vec<u8> {
    let mut coefficients = vec![0u8; degree];
    coefficients[degree - 1] = 1;
    let mut root = 1u8;
    for _ in 0..degree {
        for i in 0..degree {
            coefficients[i] = multiply(coefficients[i], root);
            if i + 1 < degree {
                coefficients[i] ^= coefficients[i + 1];
            }
        }
        root = multiply(root, 2);
    }
    coefficients
}

/// The `degree` error-correction codewords of `data`: the remainder of data * x^degree divided by the
/// generator polynomial.
pub fn remainder(data: &[u8], degree: usize) -> Vec<u8> {
    let divisor = generator(degree);
    let mut result = vec![0u8; degree];
    for &byte in data {
        let factor = byte ^ result.remove(0);
        result.push(0);
        for (slot, coefficient) in result.iter_mut().zip(&divisor) {
            *slot ^= multiply(*coefficient, factor);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplication_follows_the_field() {
        assert_eq!(multiply(0, 123), 0);
        assert_eq!(multiply(1, 123), 123);
        assert_eq!(multiply(2, 0x80), 0x1d);
        assert_eq!(multiply(0x53, 0xca), multiply(0xca, 0x53));
        // a^255 = 1: 2 raised to 255 by repeated multiplication.
        let mut power = 1u8;
        for _ in 0..255 {
            power = multiply(power, 2);
        }
        assert_eq!(power, 1);
    }

    #[test]
    fn the_degree_seven_generator_is_the_standards() {
        // ISO 18004 annex A: x^7 + a^87 x^6 + a^229 x^5 + a^146 x^4 + a^149 x^3 + a^238 x^2 + a^102 x + a^21.
        assert_eq!(generator(7), [127, 122, 154, 164, 11, 68, 117]);
    }

    #[test]
    fn the_worked_example_of_the_standard_checks_out() {
        // ISO 18004 annex I: "01234567" at version 1-M.
        let data = [
            0x10, 0x20, 0x0c, 0x56, 0x61, 0x80, 0xec, 0x11, 0xec, 0x11, 0xec, 0x11, 0xec, 0x11,
            0xec, 0x11,
        ];
        assert_eq!(
            remainder(&data, 10),
            [0xa5, 0x24, 0xd4, 0xc1, 0xed, 0x36, 0xc7, 0x87, 0x2c, 0x55]
        );
    }
}
