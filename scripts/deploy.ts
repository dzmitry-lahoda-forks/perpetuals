import 'dotenv/config.js'
import {
  deployContract,
  executeContract,
  recover,
  queryContract,
  setTimeoutDuration,
} from './helpers.js'
import { LCDClient, LocalTerra, Wallet } from '@terra-money/terra.js'
import { local, testnet } from './deploy_configs.js'
import { join } from 'path'

// consts

const MARGINED_ARTIFACTS_PATH = '../artifacts'

// main

async function main() {
  let terra: LCDClient | LocalTerra
  let wallet: Wallet
  let deployConfig: Config
  const isTestnet = process.env.NETWORK === 'testnet'

  if (process.env.NETWORK === 'testnet') {
    console.log('ghads')
    terra = new LCDClient({
      URL: 'https://bombay-lcd.terra.dev',
      chainID: 'bombay-12',
    })
    wallet = recover(terra, process.env.TEST_MAIN!)
    deployConfig = testnet
  } else {
    terra = new LocalTerra()
    wallet = (terra as LocalTerra).wallets.test1
    setTimeoutDuration(0)
    deployConfig = local
  }
  console.log(`Wallet address from seed: ${wallet.key.accAddress}`)

  /****************************************** Deploy Insurance Fund Contract *****************************************/
  console.log('Deploying Insurance Fund...')
  const insuranceFundContractAddress = await deployContract(
    terra,
    wallet,
    join(MARGINED_ARTIFACTS_PATH, 'margined_insurance_fund.wasm'),
    {},
  )
  console.log(
    'Insurance Fund Contract Address: ' + insuranceFundContractAddress,
  )

  /******************************************* Deploy Mock PriceFeed Contract *****************************************/
  console.log('Deploying Mock PriceFeed...')
  const priceFeedAddress = await deployContract(
    terra,
    wallet,
    join(MARGINED_ARTIFACTS_PATH, 'mock_pricefeed.wasm'),
    deployConfig.priceFeedInitMsg,
  )
  console.log('Mock PriceFeed Address: ' + priceFeedAddress)

  /******************************************** Deploy ETH:UST vAMM Contract ******************************************/
  console.log('Deploying ETH:UST vAMM...')
  deployConfig.vammInitMsg.pricefeed = priceFeedAddress
  const vammContractAddress = await deployContract(
    terra,
    wallet,
    join(MARGINED_ARTIFACTS_PATH, 'margined_vamm.wasm'),
    deployConfig.vammInitMsg,
  )
  console.log('ETH:UST vAMM Address: ' + vammContractAddress)

  /*************************************** Deploy Margin Engine Contract *****************************************/
  console.log('Deploy Margin Engine...')
  deployConfig.engineInitMsg.insurance_fund = insuranceFundContractAddress
  deployConfig.engineInitMsg.fee_pool = insuranceFundContractAddress // TODO this needs its own contract
  deployConfig.engineInitMsg.eligible_collateral = 'uusd' // TODO this needs its own contract
  deployConfig.engineInitMsg.vamm = [vammContractAddress]
  const marginEngineContractAddress = await deployContract(
    terra,
    wallet,
    join(MARGINED_ARTIFACTS_PATH, 'margined_engine.wasm'),
    deployConfig.engineInitMsg,
  )
  console.log('Margin Engine Address: ' + marginEngineContractAddress)

  /************************************* Define Margin engine address in vAMM *************************************/
  console.log('Set Margin Engine in vAMM...')
  await executeContract(terra, wallet, vammContractAddress, {
    update_config: {
      margin_engine: marginEngineContractAddress,
    },
  })
  console.log('margin engine set in vAMM')

  /*********************************************** Set vAMM Open ******************************************************/
  console.log('Set vAMM Open...')
  await executeContract(terra, wallet, vammContractAddress, {
    set_open: {
      open: true,
    },
  })
  console.log('vAMM set to open')

  /************************************************ Query vAMM state **********************************************/
  console.log('Querying vAMM state...')
  let state = await queryContract(terra, vammContractAddress, {
    state: {},
  })
  console.log('vAMM state:\n', state)
}

main().catch(console.log)
