// =========================================================================
// SubHub 零知识加密引擎 (Zero-Knowledge Privacy & Encryption Engine)
// 采用 AES-256-GCM 高强度认证加密与多租户密钥隔离
// =========================================================================

import crypto from 'crypto';

const ALGORITHM = 'aes-256-gcm';
const IV_LENGTH = 12; // 96 bits for GCM
const AUTH_TAG_LENGTH = 16; // 128 bits auth tag

/**
 * 为指定用户派生专属的 AES-256 加密密钥
 * @param {string} userSecret 用户密钥源 (passwordHash 或专属 salt)
 * @param {string} username 用户名
 * @returns {Buffer} 32 字节 (256 位) 加密密钥
 */
export function deriveUserKey(userSecret, username) {
  if (!userSecret || !username) {
    throw new Error('密钥派生参数缺失');
  }
  const salt = Buffer.from(`subhub_zk_tenant_${username.toLowerCase().trim()}_v1`, 'utf-8');
  // 使用 scrypt 进行高强度密钥派生 (抗 GPU/ASIC 碰撞)
  return crypto.scryptSync(String(userSecret), salt, 32);
}

/**
 * 使用 AES-256-GCM 加密用户私有配置对象
 * @param {object} plainConfig 原始明文配置对象
 * @param {Buffer} key 256 位密钥
 * @returns {object} 包含 IV, AuthTag 与密文 Payload 的加密包
 */
export function encryptUserConfig(plainConfig, key) {
  if (!plainConfig || typeof plainConfig !== 'object') {
    throw new Error('待加密配置必须为有效对象');
  }
  if (!Buffer.isBuffer(key) || key.length !== 32) {
    throw new Error('加密密钥必须为 32 字节 Buffer');
  }

  const iv = crypto.randomBytes(IV_LENGTH);
  const cipher = crypto.createCipheriv(ALGORITHM, key, iv, { authTagLength: AUTH_TAG_LENGTH });

  const jsonStr = JSON.stringify(plainConfig);
  let encryptedHex = cipher.update(jsonStr, 'utf8', 'hex');
  encryptedHex += cipher.final('hex');
  const authTagHex = cipher.getAuthTag().toString('hex');

  return {
    _encrypted: true,
    algorithm: ALGORITHM,
    iv: iv.toString('hex'),
    authTag: authTagHex,
    payload: encryptedHex,
    updatedAt: new Date().toISOString()
  };
}

/**
 * 解密 AES-256-GCM 用户私有配置包
 * @param {object} bundle 加密包或已解密的配置对象
 * @param {Buffer} key 256 位密钥
 * @returns {object} 解密后的明文配置对象
 */
export function decryptUserConfig(bundle, key) {
  if (!bundle) return null;
  // 如果已是明文对象（如旧版历史数据兼容）
  if (!bundle._encrypted) {
    return bundle;
  }

  if (!bundle.iv || !bundle.authTag || !bundle.payload) {
    throw new Error('无效的密文包格式');
  }
  if (!Buffer.isBuffer(key) || key.length !== 32) {
    throw new Error('解密密钥必须为 32 字节 Buffer');
  }

  const iv = Buffer.from(bundle.iv, 'hex');
  const authTag = Buffer.from(bundle.authTag, 'hex');
  const decipher = crypto.createDecipheriv(ALGORITHM, key, iv, { authTagLength: AUTH_TAG_LENGTH });
  decipher.setAuthTag(authTag);

  let decryptedStr = decipher.update(bundle.payload, 'hex', 'utf8');
  decryptedStr += decipher.final('utf8');

  return JSON.parse(decryptedStr);
}

/**
 * 校验一个对象是否为密文包
 * @param {object} obj
 * @returns {boolean}
 */
export function isEncryptedBundle(obj) {
  return !!(obj && typeof obj === 'object' && obj._encrypted === true && obj.payload);
}
