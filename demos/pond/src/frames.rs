/// The six sides of a frame, as the cube's +X, -X, +Y, -Y, +Z, -Z.
macro_rules! files {
    ($($n:literal)*) => {
        &[
            $(
                concat!("dscn0", $n, "_right.dds"),
                concat!("dscn0", $n, "_left.dds"),
                concat!("dscn0", $n, "_top.dds"),
                concat!("dscn0", $n, "_bottom.dds"),
                concat!("dscn0", $n, "_front.dds"),
                concat!("dscn0", $n, "_back.dds"),
            )*
            "pool.dds",
            "pool_bump.tga",
            "dirt.dds",
            "shadow.dds",
            "logo.dds",
            "help.dds",
        ]
    };
}
/// Photographs of a day, every other number.
#[rustfmt::skip]
pub const FILES: &[&str] = files! {
    123 125 127 129 131 133 135 137 139 141 143 145 147 149 151 153 155 157 159 161
    163 165 167 169 171 173 175 177 179 181 183 185 187 189 191 193 195 197 199 201
    203 205 207 209 211 213 215 217 219 221 223 225 227 229 231 233 235 237 239 241
    243 245 247 249 251 253 255 257 259 261 263 265 267 269 271 273 275 277 279 281
    283 285 287 289 291 293 295 297 299 301 303 305 307 309 311 313 315 317 319 321
    323 325 327 329 331 333 335 337 339 341 343 345 347 349 351 353 355
};
